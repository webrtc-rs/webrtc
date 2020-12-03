use criterion::criterion_main;

mod marshal;
mod unmarshal;

criterion_main!(unmarshal::unmarshal, marshal::marshal);
