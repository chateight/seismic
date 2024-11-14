use chrono::Utc;
use csv::ReaderBuilder;
use plotters::prelude::*;
use std::fs::File;
use std::io;
use std::thread;
use std::time::Duration;

const N: usize = 12000;

const TARGET_FPS: u32 = 200; // frame rate
//const AD2GAL: f32 = 1.13426; // correction value from ADC to Gal
const AD2GAL: f32 = 1.0;

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

    fn update(&mut self, raw_adc: [f32; 3], frame: u32) {
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
                self.rc_values[i] * 0.94 + self.adc_values[self.adc_values.len() - 1][i] * 0.06;        // modify
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

        if min_a > 0.0 {
            2.0 * min_a.log10() + 0.94
        } else {
            0.0
        }
    }
}

fn draw(
    x: Vec<usize>,
    y: Vec<f32>,
    f_name: &str,
    cap: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let image_width = 1080;
    let image_height = 360;

    let root = BitMapBackend::new(f_name, (image_width, image_height)).into_drawing_area();

    root.fill(&WHITE)?;

    //   https://qiita.com/lo48576/items/343ca40a03c3b86b67cb
    let (y_min, y_max) = y
        .iter()
        .fold((0.0 / 0.0, 0.0 / 0.0), |(m, n), v| (v.min(m), v.max(n)));

    let caption = cap;
    let font = ("sans-serif", 20);

    let mut chart = ChartBuilder::on(&root)
        .caption(caption, font.into_font())
        .margin(10)
        .x_label_area_size(16)
        .y_label_area_size(42)
        .build_cartesian_2d(*x.first().unwrap()..*x.last().unwrap(), y_min..y_max)?;

    chart.configure_mesh().draw()?;

    let line_series = LineSeries::new(x.iter().zip(y.iter()).map(|(x, y)| (*x, *y)), &RED);
    chart.draw_series(line_series)?;

    Ok(())
}

fn read_csv_to_2d_array(path: &str) -> io::Result<Vec<[f32; 3]>> {
    let file = File::open(path)?;
    let mut reader = ReaderBuilder::new()
        .has_headers(false) // ヘッダー行がない場合
        .from_reader(file);

    let mut data: Vec<[f32; 3]> = Vec::new();

    for result in reader.records() {
        let record = result?;
        let mut row_data: Vec<f32> = Vec::new();

        for field in record.iter() {
            if let Ok(value) = field.parse::<f32>() {
                row_data.push(value);
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Failed to parse f32 value",
                ));
            }
        }

        if row_data.len() != 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected 3 columns but found different number",
            ));
        }

        let array: [f32; 3] = [row_data[0], row_data[1], row_data[2]];
        data.push(array);
    }

    Ok(data)
}

fn main() -> io::Result<()> {
    let mut seismic_data = SeismicData::new();
    let start_time = Utc::now();
    let mut frame = 0;

    let path = "./noto.csv";
    let mut seismic_calc = Vec::new();
    let slice_vec = read_csv_to_2d_array(path);

    for row in slice_vec? {
        let slice = [
            *row.get(0).unwrap(),
            *row.get(1).unwrap(),
            *row.get(2).unwrap(),
        ];

        seismic_data.update(slice, frame);

        if frame == 4000{
            println!("slice : {:?}", slice);
        }

        seismic_calc.push(seismic_data.calculate_seismic_scale());

        if frame % (TARGET_FPS) == 0 {
            let current_time = Utc::now();
            println!(
                "time: {}, scale: {}, frame: {}",
                current_time,
                seismic_data.calculate_seismic_scale(),
                frame
            );
        }

        frame += 1;
        let next_frame_time = frame as f32 / (TARGET_FPS as f32);
        let elapsed_time = (Utc::now() - start_time).num_milliseconds() as f32 / 1000.0;
        let remain_time = next_frame_time - elapsed_time;

        if remain_time > 0.0 {
            //thread::sleep(Duration::from_millis((remain_time * 1000.0) as u64));
        }
    }

    // draw seismic output
    let buf_re = seismic_calc.clone();

    let n = N;
    let f_input = "seismic.png";
    let cap = "seismic output";
    let x: Vec<usize> = (1..=n).collect();
    let _ = draw(x, buf_re, &f_input, &cap);

    Ok(())
}
