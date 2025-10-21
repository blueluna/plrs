use clap::{Arg, Command};
use env_logger;
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

    let mut fifo =
        logik_xilinx::StreamFifo::try_from(&device, logik_xilinx::StreamFifoDataWidth::Bits64)
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
        _ => unreachable!("Invalid configuration"),
    }
    ExitCode::SUCCESS
}
