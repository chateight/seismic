use chrono::Utc;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use linux_embedded_hal::I2cdev;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

fn disp_ch(val: f32) {
    let (tx, rx) = channel();
    let _receiver = thread::spawn(move || {
        let value: String = rx.recv().expect("Unable to receive from channel");
        disp(value.clone());
    });

    let rounded_val = (val * 10.0).round() / 10.0;
    tx.send(rounded_val.to_string())
        .expect("Unable to send on channel");
}

fn disp(value: String) {
    let i2c = I2cdev::new("/dev/i2c-1").unwrap();

    let interface = I2CDisplayInterface::new(i2c);
    let mut disp = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    disp.init().unwrap();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BinaryColor::On)
        .build();

    let mag = format!("Val : {}", value);

    Text::with_baseline(
        &format!("{}", mag),
        Point::new(15, 20),
        text_style,
        Baseline::Top,
    )
    .draw(&mut disp)
    .unwrap();

    disp.flush().unwrap();
}

const TARGET_FPS: u32 = 200; // frame rate
// 660mV/G * 4,096/3,300 * k = 980 -> k = 1.196
//const AD2GAL: f32 = 1.13426; // correction value from ADC to Gal
const AD2GAL: f32 = 1.196;

struct SeismicData {
    adc_values: Vec<[f32; 3]>, // raw data ring buffer size TARGET_FPS
    rc_values: [f32; 3],       // acceleration data (temporary)
    a_values: Vec<f32>,        // acceleration ring buffer size TARGET_FPS*5
}

impl SeismicData {
    fn new() -> Self {
        SeismicData {
            adc_values: Vec::new(),
            rc_values: [0.0; 3],
            a_values: Vec::new(),
        }
    }

    fn update(&mut self, raw_adc: [f32; 3]) {
        if self.adc_values.len() >= TARGET_FPS as usize {
            self.adc_values.remove(0);
        }
        self.adc_values.push(raw_adc);

        let mut axis_gals = [0.0, 0.0, 0.0];

        for i in 0..3 {
            let mut sum = 0.0;
            for adc in &self.adc_values {
                sum += adc[i];
            }
            let offset = sum / TARGET_FPS as f32;

            self.rc_values[i] =
                self.rc_values[i] * 0.94 + self.adc_values[self.adc_values.len() - 1][i] * 0.06;
            axis_gals[i] = (self.rc_values[i] - offset) * AD2GAL;
        }

        let composite_gal =
            (axis_gals[0].powi(2) + axis_gals[1].powi(2) + axis_gals[2].powi(2)).sqrt();
        if self.a_values.len() >= TARGET_FPS as usize * 5 {
            self.a_values.remove(0);
        }
        self.a_values.push(composite_gal);
    }

    fn calculate_seismic_scale(&self) -> f32 {
        let a_frame = (TARGET_FPS as f32 * 0.3) as usize;
        let mut min_a: f32 = 0.0;
        let mut a_values_clone = self.a_values.clone();
        a_values_clone.sort_by(|a, b| a.partial_cmp(b).unwrap());

        if a_values_clone.len() > a_frame {
            min_a = a_values_clone[self.a_values.len() - a_frame];
        }

        if min_a > 0.4 {
            2.0 * min_a.log10() + 0.94
        } else {
            0.0
        }
    }
}

fn adc_read() -> Result<[u16; 3], String> {
    const BYTE0: u8 = 0x06 | 0x00; // Command byte for channel x in single-ended mode

    // Create a SPI object
    let spi = match Spi::new(Bus::Spi0, SlaveSelect::Ss0, 1_000_000, Mode::Mode0) {
        Ok(spi) => spi,
        Err(e) => return Err(format!("Failed to create SPI object: {}", e)),
    };

    // Define the buffers for reading
    let mut read_buffer = [0u8; 3];

    // Channel array for write_buffer
    let ch_array = [0x00u8, 0x40u8, 0x80u8];
    let mut result = [0; 3];

    for i in 0..=2 {
        let write_buffer = [BYTE0, ch_array[i], 0x00]; // Command stream (3 bytes)
                                                       // Perform the SPI transfer
        match spi.transfer(&mut read_buffer, &write_buffer) {
            Ok(_) => (),
            Err(e) => return Err(format!("SPI transfer failed: {}", e)),
        }

        // Extract the 12-bit ADC value
        // Note: MCP3204 returns 12-bit data, but the second byte contains status bit(0) and the MSB of the result.
        let msb = (read_buffer[1] & 0x0F) as u16;
        let lsb = read_buffer[2] as u16; // Convert third byte to u16
        result[i] = (msb << 8) | lsb;
    }
    Ok(result)
}

fn main() {
    let mut seismic_data = SeismicData::new();
    let mut start_time = Utc::now();
    let mut frame = 0;

    loop {
        match adc_read() {
            Ok(raw_adc) => seismic_data.update(raw_adc.map(|x| x as f32)),
            Err(e) => {
                eprintln!("Error reading ADC: {}", e);
                continue;
            }
        }

        if frame % (TARGET_FPS) == 0 {
            disp_ch(seismic_data.calculate_seismic_scale());
        }

        frame += 1;
        let next_frame_time = frame as f32 / (TARGET_FPS as f32);
        let elapsed_time = (Utc::now() - start_time).num_milliseconds() as f32 / 1000.0;
        let remain_time = next_frame_time - elapsed_time;

        if remain_time > 0.0 {
            thread::sleep(Duration::from_millis((remain_time * 1000.0) as u64));
        }

        if frame >= 2_147_483_647 {
            start_time = Utc::now();
            frame = 1;
        }
    }
}
