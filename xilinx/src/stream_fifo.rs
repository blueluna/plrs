use crate::error::Error;
use uio_rs;

pub enum StreamFifoDataWidth {
    Bits32,
    Bits64,
    Bits128,
    Bits256,
    Bits512,
}

impl StreamFifoDataWidth {
    pub fn byte_count(&self) -> usize {
        match self {
            Self::Bits32 => size_of::<u32>(),
            Self::Bits64 => size_of::<u64>(),
            Self::Bits128 => size_of::<u128>(),
            Self::Bits256 => 32,
            Self::Bits512 => 64,
        }
    }
}

pub struct StreamFifo<'a> {
    data_width: StreamFifoDataWidth,
    axi_lite: uio_rs::Map<'a>,
    axi: Option<uio_rs::Map<'a>>,
}

impl<'a> StreamFifo<'a> {
    pub fn try_from_lite(device: &'a uio_rs::Device) -> Result<StreamFifo<'a>, Error> {
        let axi_lite = uio_rs::Map::new(device, 0)?;
        Ok(StreamFifo {
            data_width: StreamFifoDataWidth::Bits32,
            axi_lite,
            axi: None,
        })
    }

    pub fn try_from(
        device: &'a uio_rs::Device,
        data_width: StreamFifoDataWidth,
    ) -> Result<StreamFifo<'a>, Error> {
        let map_descriptions = device.maps();
        if map_descriptions.len() >= 2 {
            let axi_lite = uio_rs::Map::new(device, 0)?;
            let axi = uio_rs::Map::new(device, 1)?;
            Ok(StreamFifo {
                data_width,
                axi_lite,
                axi: Some(axi),
            })
        } else if map_descriptions.len() == 1 {
            let axi_lite = uio_rs::Map::new(device, 0)?;
            Ok(StreamFifo {
                data_width: StreamFifoDataWidth::Bits32,
                axi_lite,
                axi: None,
            })
        } else {
            Err(Error::OutOfBound)
        }
    }

    pub fn reset(&mut self) -> Result<(), Error> {
        self.axi_lite
            .write_u32(REG_AXI4_STREAM_RESET, RESET_MAGIC)?;
        self.axi_lite.write_u32(REG_TX_RESET, RESET_MAGIC)?;
        self.axi_lite.write_u32(REG_RX_RESET, RESET_MAGIC)?;
        self.axi_lite.write_u32(
            REG_INTERRUPT_ENABLE,
            INTERRUPT_TX_COMPLETE
                | INTERRUPT_RX_COMPLETE
                | INTERRUPT_RX_UNDER_READ
                | INTERRUPT_RX_OVER_READ
                | INTERRUPT_RX_UNDER_RUN
                | INTERRUPT_TX_OVER_RUN
                | INTERRUPT_TX_LENGTH_MISMATCH,
        )?;
        self.interrupts_clear()?;
        Ok(())
    }

    pub fn interrupts_clear(&mut self) -> Result<(), Error> {
        self.axi_lite
            .write_u32(REG_INTERRUPT_STATUS, INTERRUPT_ALL)
            .map_err(|e| e.into())
    }

    pub fn interrupts_clear_rx(&mut self) -> Result<(), Error> {
        self.axi_lite
            .write_u32(
                REG_INTERRUPT_STATUS,
                INTERRUPT_RX_ERROR | INTERRUPT_RX_COMPLETE,
            )
            .map_err(|e| e.into())
    }

    pub fn read(&mut self, words: &mut [u32]) -> Result<(usize, u8), Error> {
        let occupancy = self.axi_lite.read_u32(REG_RX_OCCUPANCY)?;
        if occupancy == 0 {
            return Err(Error::Empty);
        }
        // REG_RX_DATA and REG_RX_LENGTH seems to fail
        // with bus error if there has been no transfer.
        self.interrupts_clear_rx()?;
        let packet_bytes = (self.axi_lite.read_u32(REG_RX_LENGTH)? & 0x003fffff) as usize;
        let words_bytes = words.len() * size_of::<u32>();
        let read_bytes = words_bytes.min(packet_bytes);
        let read_words = read_bytes / size_of::<u32>();
        let destination = self.axi_lite.read_u32(REG_RX_DESTINATION)? as u8;
        log::debug!(
            "Occupancy {} Receive {} bytes {} bytes {} words expected ",
            occupancy,
            packet_bytes,
            read_bytes,
            words.len()
        );

        if let Some(ref axi) = self.axi {
            let fifo_word_size = self.data_width.byte_count();
            let read_count = read_bytes / fifo_word_size;
            let mut target_index = 0;
            log::debug!("AXI {} count {} bytes", read_count, fifo_word_size);
            for n in 0..read_count {
                match axi.read_exact(FULL_REG_READ, fifo_word_size) {
                    Ok(fifo_chunk) => match self.data_width {
                        StreamFifoDataWidth::Bits32 => {
                            let value = u32::from_ne_bytes(fifo_chunk.try_into().unwrap());
                            words[target_index] = value;
                            target_index += 1;
                        }
                        StreamFifoDataWidth::Bits64 => {
                            let value = u64::from_ne_bytes(fifo_chunk.try_into().unwrap());
                            words[target_index] = ((value & 0xffffffff00000000) >> 32) as u32;
                            words[target_index + 1] = (value & 0x00000000ffffffff) as u32;
                            target_index += 2;
                        }
                        StreamFifoDataWidth::Bits128 => {
                            let value = u128::from_ne_bytes(fifo_chunk.try_into().unwrap());
                            words[target_index] =
                                ((value & 0xffffffff000000000000000000000000) >> 96) as u32;
                            words[target_index + 1] =
                                ((value & 0x00000000ffffffff0000000000000000) >> 64) as u32;
                            words[target_index + 2] =
                                ((value & 0x0000000000000000ffffffff00000000) >> 32) as u32;
                            words[target_index + 3] =
                                (value & 0x000000000000000000000000ffffffff) as u32;
                            target_index += 4;
                        }
                        StreamFifoDataWidth::Bits256 => {
                            log::debug!("AXI {:>3} {} bytes", n, fifo_word_size);
                            target_index += 8;
                        }
                        StreamFifoDataWidth::Bits512 => {
                            log::debug!("AXI {:>3} {} bytes", n, fifo_word_size);
                            target_index += 16;
                        }
                    },
                    Err(ref error) => {
                        log::warn!("Failed to read AXI chunk, {:?}", error);
                    }
                }
            }
        } else {
            log::debug!("AXI-lite {} words", read_words);
            for w in &mut words[..read_words] {
                let v = self.axi_lite.read_u32(REG_RX_DATA)?;
                log::debug!("Receive {:08x}", v);
                *w = v;
            }
        }
        let interrupts = self.axi_lite.read_u32(REG_INTERRUPT_STATUS)?;
        if (interrupts & INTERRUPT_RX_ERROR) != 0 {
            log::warn!("Receive error, {:08x}", interrupts);
            self.reset()?;
            return Err(Error::Address);
        }
        Ok((read_words, destination))
    }
}

/// AXI Stream FIFO reset word
const RESET_MAGIC: u32 = 0x000000A5;

// AXI-lite registers
const REG_INTERRUPT_STATUS: usize = 0x00;
const REG_INTERRUPT_ENABLE: usize = 0x04;
const REG_TX_RESET: usize = 0x08;
const REG_TX_VACANCY: usize = 0x0c;
const REG_TX_DATA: usize = 0x10;
const REG_TX_LENGTH: usize = 0x14;
/// Receiver reset
const REG_RX_RESET: usize = 0x18;
/// Receiver occupancy, number of location used for data storage
const REG_RX_OCCUPANCY: usize = 0x1c;
/// Data register, where the FIFO is read
const REG_RX_DATA: usize = 0x20;
/// Receive length register, number of bytes in the next "packet"
const REG_RX_LENGTH: usize = 0x24;

const REG_AXI4_STREAM_RESET: usize = 0x28;
const REG_TX_DESTINATION: usize = 0x2c;
const REG_RX_DESTINATION: usize = 0x30;

// AXI4 registers
const FULL_REG_WRITE: usize = 0x00000000;
const FULL_REG_READ: usize = 0x00001000;

// Interrupts
/// Receive under-read interrupt
const INTERRUPT_RX_UNDER_READ: u32 = 0x80000000;
/// Receive over-read interrupt
const INTERRUPT_RX_OVER_READ: u32 = 0x40000000;
/// Receive under run (empty) interrupt
const INTERRUPT_RX_UNDER_RUN: u32 = 0x20000000;
/// Transmit overrun interrupt
const INTERRUPT_TX_OVER_RUN: u32 = 0x10000000;
/// Transmit complete interrupt
const INTERRUPT_TX_COMPLETE: u32 = 0x08000000;
/// Receive complete interrupt
const INTERRUPT_RX_COMPLETE: u32 = 0x04000000;
/// Transmit length mismatch interrupt
const INTERRUPT_TX_LENGTH_MISMATCH: u32 = 0x02000000;
/// Transmit reset complete interrupt
const INTERRUPT_TX_RESET_COMPLETE: u32 = 0x01000000;
/// Receive reset complete interrupt
const INTERRUPT_RX_RESET_COMPLETE: u32 = 0x00800000;
/// Tx FIFO Programmable Full interrupt
const INTERRUPT_TX_PROGRAMMABLE_FULL: u32 = 0x00400000;
/// Tx FIFO Programmable Empty interrupt
const INTERRUPT_TX_PROGRAMMABLE_EMPTY: u32 = 0x00200000;
/// Rx FIFO Programmable Full interrupt
const INTERRUPT_RX_PROGRAMMABLE_FULL: u32 = 0x00100000;
/// Rx FIFO Programmable Empty interrupt
const INTERRUPT_RX_PROGRAMMABLE_EMPTY: u32 = 0x00080000;
/// All interrupts
const INTERRUPT_ALL: u32 = INTERRUPT_RX_PROGRAMMABLE_EMPTY
    | INTERRUPT_RX_PROGRAMMABLE_FULL
    | INTERRUPT_TX_PROGRAMMABLE_EMPTY
    | INTERRUPT_TX_PROGRAMMABLE_FULL
    | INTERRUPT_RX_RESET_COMPLETE
    | INTERRUPT_TX_RESET_COMPLETE
    | INTERRUPT_TX_LENGTH_MISMATCH
    | INTERRUPT_RX_COMPLETE
    | INTERRUPT_TX_COMPLETE
    | INTERRUPT_TX_OVER_RUN
    | INTERRUPT_RX_UNDER_RUN
    | INTERRUPT_RX_OVER_READ
    | INTERRUPT_RX_UNDER_READ;
/// Error status interrupts
const INTERRUPT_ERROR: u32 = INTERRUPT_RX_ERROR | INTERRUPT_TX_ERROR;
/// Receive Error status interrupts
const INTERRUPT_RX_ERROR: u32 =
    INTERRUPT_RX_UNDER_RUN | INTERRUPT_RX_OVER_READ | INTERRUPT_RX_UNDER_READ;
/// Transmit Error status interrupts
const INTERRUPT_TX_ERROR: u32 = INTERRUPT_TX_OVER_RUN | INTERRUPT_TX_LENGTH_MISMATCH;
