Raspberry pi zero seismic measurement tool based on following link

https://greensoybean.hatenablog.com/search?q=地震計


- Japan Meteorological Agency recomended filtering characteristics : 震度計算のためのフィルタ特性.png
- disp1.py : calc result disply script(assuming value.txt file for temporary storage
- seismic.py : main routine for calculating seismic value
- seismic_sch.png : circuit schematics
- jpg file : outer view of the measurement tool without shutdown switch


Rust_version folder contains codes with Rust 

- main.rs : console output version

- main_disp : display on the ssd1306

- seismic_sim : calcurate using the open data as follows
(https://www.data.jma.go.jp/eqev/data/kyoshin/jishin/2401011610_noto/index.html)
edit the download file to delete the header portion

-Cargo.toml : current final version for main_disp.rs, you need to edit that file in case of compiling other source files
