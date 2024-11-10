use chrono::Utc;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
use std::thread;
use std::time::{Duration, Instant};

const TARGET_FPS: u32 = 200; // frame rate
const AD2GAL: f32 = 1.13426; // correction value from ADC to Gal

struct SeismicData {
    adc_values: Vec<[f32; 3]>, // raw data ring buufer size TARGET_FPS
    rc_values: [f32; 3],       // accelaration data(temporary)
    a_values: Vec<f32>,        // accelaration ring buffer size TARGET_FPS*5
    adc_ring_index: usize,     // adc_values ring budder index Max TARGET_FPS
    a_ring_index: usize,       // accelaration ring buffer index Max TARGET_FPS*5
}

impl SeismicData {
    fn new() -> Self {
        SeismicData {
            adc_values: Vec::new(),
            rc_values: [0.0; 3],
            a_values: Vec::new(),
            adc_ring_index: 0,
            a_ring_index: 0,
        }
    }

    fn update(&mut self, raw_adc: [f32; 3]) {
        if self.adc_values.len() >= TARGET_FPS as usize {
            // use vector as a ring buffer
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
                self.rc_values[i] * 0.94 + self.adc_values[self.adc_ring_index][i] * 0.06;
            axis_gals[i] = (self.rc_values[i] - offset) * AD2GAL;
        }

        let composite_gal =
            (axis_gals[0].powi(2) + axis_gals[1].powi(2) + axis_gals[2].powi(2)).sqrt();
        if self.a_values.len() >= TARGET_FPS as usize * 5 {
            self.a_values.remove(0);
        }
        self.a_values.push(composite_gal);

        self.adc_ring_index = (self.adc_ring_index + 1) % (TARGET_FPS as usize);
        self.a_ring_index = (self.a_ring_index + 1) % ((TARGET_FPS as usize) * 5);
    }

    fn calculate_seismic_scale(&self) -> f32 {
        let a_frame = (TARGET_FPS as f32 * 0.3) as usize;
        let mut min_a: f32 = 0.0;
        let mut a_values_clone = self.a_values.clone();
        a_values_clone.sort_by(|a, b| a.partial_cmp(b).unwrap());

        if a_values_clone.len() > a_frame {
            min_a = a_values_clone[self.a_values.len() - a_frame];
        }

        if min_a > 0.0 {
            2.0 * min_a.log10() + 0.94
        } else {
            0.0
        }
    }
}

fn adc_read() -> [u16; 3] {
    const BYTE0: u8 = 0x06 | 0x00; // Command byte for channel x in single-ended mode

    // Create a SPI object
    let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 1_000_000, Mode::Mode0).unwrap();

    // Define the buffers for reading
    let mut read_buffer = [0u8; 3];

    // channel array for write_buffer
    let ch_array = [0x00u8, 0x40u8, 0x80u8];
    let mut result = [0; 3];

    for i in 0..=2 {
        let write_buffer = [BYTE0, ch_array[i], 0x00]; // command stream(3 bytes)
                                                       // Perform the SPI transfer
        spi.transfer(&mut read_buffer, &write_buffer).unwrap();

        // Extract the 12-bit ADC value
        // Note: MCP3204 returns 12-bit data, but the second byte contains status bit(0) and the MSB of the result.
        let msb = (read_buffer[1] & 0x0F) as u16;
        let lsb = read_buffer[2] as u16; // Convert third byte to u16
        result[i] = (msb << 8) | lsb;
    }
    result
}

fn main() {
    let mut seismic_data = SeismicData::new();
    let mut start_time = Instant::now();
    let mut frame = 0;

    loop {

        seismic_data.update(adc_read().map(|x| x as f32));

        if frame % (TARGET_FPS / 10) == 0 {
            println!(
                "time: {}, scale: {}, frame: {}",
                Utc::now(),
                seismic_data.calculate_seismic_scale(),
                frame
            );
        }

        frame += 1;
        let next_frame_time = frame as f32 / (TARGET_FPS as f32);
        let current_time = start_time.elapsed().as_secs_f32();
        let remain_time = next_frame_time - current_time;

        if remain_time > 0.0 {
            thread::sleep(Duration::from_secs_f32(remain_time));
        }

        if frame >= 2_147_483_647 {
            start_time = Instant::now();
            frame = 1;
        }
    }
}