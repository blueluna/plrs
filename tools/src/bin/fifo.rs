use clap::{Arg, Command};
use env_logger;
use logik_xilinx::StreamFifoValue;
use std::process::ExitCode;
use uio_rs::{self, Device};

fn main() -> ExitCode {
    env_logger::init();

    let cmd = Command::new("fifo")
        .bin_name("fifo")
        .arg(
            Arg::new("device")
                .short('d')
                .long("device")
                .required(true)
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("interrupt")
                .short('i')
                .long("interrupt")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(
            Command::new("read").about("Read from the FIFO").arg(
                Arg::new("size")
                    .index(1)
                    .value_parser(clap::value_parser!(usize))
                    .action(clap::ArgAction::Set)
                    .required(true),
            ),
        )
        .subcommand(
            Command::new("write")
                .about("Write to the FIFO")
                .arg(
                    Arg::new("size")
                        .index(1)
                        .value_parser(clap::value_parser!(usize))
                        .action(clap::ArgAction::Set)
                        .required(true),
                )
                .arg(
                    Arg::new("value")
                        .index(2)
                        .action(clap::ArgAction::Set)
                        .required(true),
                ),
        );
    let matches = cmd.get_matches();
    let device_name: &String = matches.get_one("device").unwrap();

    let devices = uio_rs::DeviceDescription::enumerate();
    let device = devices.iter().find(|d| d.name() == device_name);
    let uio_number = if let Some(device) = device {
        device.uio()
    } else {
        if let Ok(n) = device_name.parse::<u16>() {
            n
        } else {
            u16::MAX
        }
    };

    if uio_number == u16::MAX {
        eprintln!("Failed to find UIO device {}", device_name);
        return ExitCode::FAILURE;
    }

    let mut device = Device::new(uio_number).expect("Failed to open UIO device");

    if *matches.get_one("interrupt").unwrap() {
        device
            .interrupt_enable()
            .expect("Failed to enable interrupt");
        let value = device
            .interrupt_wait()
            .expect("Failed to wait for interrupt");
        println!("Interrupt {}", value);
    }

    let mut fifo = logik_xilinx::StreamFifo::try_from(&device, logik_xilinx::StreamFifoValue::U64)
        .expect("Failed to load FIFO");

    match matches.subcommand() {
        Some(("read", cmd)) => {
            if let Some(size) = cmd.get_one("size") {
                let mut block = vec![0u32; *size];
                match fifo.read(&mut block) {
                    Ok((words, destination)) => {
                        for w in &block[..words] {
                            println!("{:08x}", w);
                        }
                        println!("destination {:02x}", destination);
                    }
                    Err(ref error) => {
                        eprintln!("FIFO read failed {:?}", error);
                    }
                }
            }
        }
        Some(("write", cmd)) => {
            if let (Some(size), Some(text)) =
                (cmd.get_one::<usize>("size"), cmd.get_one::<String>("value"))
            {
                let data_width = fifo.data_width();
                let bytes = size * data_width.byte_count();
                let mut block = vec![0u8; bytes];
                match data_width {
                    StreamFifoValue::U32 => {
                        let v = {
                            if text.starts_with("0x") {
                                let (_, hex) = text.split_at(2);
                                u32::from_str_radix(hex, 16).unwrap_or(0)
                            } else {
                                u32::from_str_radix(text, 10).unwrap_or(0)
                            }
                        };
                        let mut write_value = v;
                        for n in 0..*size {
                            let offset = n * data_width.byte_count();
                            let part = &mut block[offset..offset + data_width.byte_count()];
                            part.copy_from_slice(&write_value.to_ne_bytes());
                            write_value = write_value.wrapping_add(1);
                        }
                    }
                    StreamFifoValue::U64 => {
                        let v = {
                            if text.starts_with("0x") {
                                let (_, hex) = text.split_at(2);
                                u64::from_str_radix(hex, 16).unwrap_or(0)
                            } else {
                                u64::from_str_radix(text, 10).unwrap_or(0)
                            }
                        };
                        let mut write_value = v;
                        for n in 0..*size {
                            let offset = n * data_width.byte_count();
                            let part = &mut block[offset..offset + data_width.byte_count()];
                            part.copy_from_slice(&write_value.to_ne_bytes());
                            write_value = write_value.wrapping_add(1);
                        }
                    }
                    StreamFifoValue::U128 => {
                        let v = {
                            if text.starts_with("0x") {
                                let (_, hex) = text.split_at(2);
                                u128::from_str_radix(hex, 16).unwrap_or(0)
                            } else {
                                u128::from_str_radix(text, 10).unwrap_or(0)
                            }
                        };
                        let mut write_value = v;
                        for n in 0..*size {
                            let offset = n * data_width.byte_count();
                            let part = &mut block[offset..offset + data_width.byte_count()];
                            part.copy_from_slice(&write_value.to_ne_bytes());
                            write_value = write_value.wrapping_add(1);
                        }
                    }
                    StreamFifoValue::U256 => {
                        eprintln!("256-bit not implemented");
                    }
                    StreamFifoValue::U512 => {
                        eprintln!("512-bit not implemented");
                    }
                }
                fifo.write(&block, 0).expect("Failed to write to FIFO");
            }
        }
        _ => unreachable!("Invalid configuration"),
    }
    ExitCode::SUCCESS
}
