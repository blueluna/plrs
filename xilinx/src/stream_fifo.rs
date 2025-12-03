#[deny(missing_docs)]
use crate::error::Error;
use uio_rs;

/// Supported data widths for the AXI Stream FIFO
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StreamFifoValue {
    U32,
    U64,
    U128,
    U256,
    U512,
}

impl StreamFifoValue {
    /// Returns the byte count of the data width.
    pub fn byte_count(&self) -> usize {
        match self {
            Self::U32 => size_of::<u32>(),
            Self::U64 => size_of::<u64>(),
            Self::U128 => size_of::<u128>(),
            Self::U256 => 32,
            Self::U512 => 64,
        }
    }
    /// Returns the byte count of the data width.
    pub fn try_from_bits(bits: usize) -> Option<Self> {
        match bits {
            32 => Some(Self::U32),
            64 => Some(Self::U64),
            128 => Some(Self::U128),
            256 => Some(Self::U256),
            512 => Some(Self::U512),
            _ => None,
        }
    }
}

/// Represents an AXI Stream FIFO device.
pub struct StreamFifo {
    data_width: StreamFifoValue,
    axi_lite: uio_rs::Map,
    axi: Option<uio_rs::Map>,
}

impl StreamFifo {

    /// Creates a new `StreamFifo` instance from a UIO device.
    pub fn try_from(
        device: &uio_rs::Device,
        data_width: StreamFifoValue,
    ) -> Result<StreamFifo, Error> {
        let map_descriptions = device.maps();
        if map_descriptions.len() >= 2 {
            let axi_lite = uio_rs::Map::try_from_device(device, 0)?;
            let axi = uio_rs::Map::try_from_device(device, 1)?;
            Ok(StreamFifo {
                data_width,
                axi_lite,
                axi: Some(axi),
            })
        } else if map_descriptions.len() == 1 {
            let axi_lite = uio_rs::Map::try_from_device(device, 0)?;
            Ok(StreamFifo {
                data_width: StreamFifoValue::U32,
                axi_lite,
                axi: None,
            })
        } else {
            Err(Error::NoMemoryMap)
        }
    }

    /// Returns the data width of the FIFO.
    pub fn data_width(&self) -> StreamFifoValue {
        self.data_width
    }

    /// Resets the AXI Stream FIFO.
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

    /// Clears all interrupts for the AXI Stream FIFO.
    pub fn interrupts_clear(&mut self) -> Result<(), Error> {
        self.axi_lite
            .write_u32(REG_INTERRUPT_STATUS, INTERRUPT_ALL)
            .map_err(|e| e.into())
    }

    /// Clears all RX interrupts for the AXI Stream FIFO.
    pub fn interrupts_clear_rx(&mut self) -> Result<(), Error> {
        self.axi_lite
            .write_u32(
                REG_INTERRUPT_STATUS,
                INTERRUPT_RX_ERROR | INTERRUPT_RX_COMPLETE,
            )
            .map_err(|e| e.into())
    }

    /// Clears all TX interrupts for the AXI Stream FIFO.
    pub fn interrupts_clear_tx(&mut self) -> Result<(), Error> {
        self.axi_lite
            .write_u32(
                REG_INTERRUPT_STATUS,
                INTERRUPT_TX_ERROR | INTERRUPT_TX_COMPLETE,
            )
            .map_err(|e| e.into())
    }

    /// Reads bytes from the AXI Stream FIFO.
    pub fn read_bytes(&mut self, data: &mut [u8]) -> Result<(usize, u8), Error> {
        let occupancy = self.axi_lite.read_u32(REG_RX_OCCUPANCY)?;
        if occupancy == 0 {
            return Err(Error::Empty);
        }
        // REG_RX_DATA and REG_RX_LENGTH seems to fail
        // with bus error if there has been no transfer.
        self.interrupts_clear_rx()?;
        let packet_bytes = (self.axi_lite.read_u32(REG_RX_LENGTH)? & 0x003fffff) as usize;
        let read_bytes = data.len().min(packet_bytes);
        let destination = self.axi_lite.read_u32(REG_RX_DESTINATION)? as u8;
        log::debug!(
            "Occupancy {} Receive {} bytes {} bytes {} bytes expected ",
            occupancy,
            packet_bytes,
            read_bytes,
            data.len()
        );
        let fifo_word_size = self.data_width.byte_count();
        let read_count = (read_bytes + (fifo_word_size - 1)) / fifo_word_size;

        // This access is hard to get right without getting double or more reads on the register for each call.
        // The following reasons that this is because of the memcpy call in arm64 libc.
        // https://adaptivesupport.amd.com/s/question/0D54U00008Z19O5SAJ/why-are-my-uio-accesses-from-python-being-done-twice-in-the-logic-using-petalinuxvivado-20241?language=en_US
        // To convert the memory mapped byte slice to a u32 seems to work in this case...

        if let Some(ref axi) = self.axi {
            for n in 0..read_count {
                let offset = n * fifo_word_size;
                let fifo_chunk = axi.read_exact(FULL_REG_READ, fifo_word_size)?;
                match self.data_width() {
                    StreamFifoValue::U32 => {
                        let v = u32::from_ne_bytes(fifo_chunk.try_into().unwrap());
                        data[offset..offset + fifo_word_size].copy_from_slice(&v.to_ne_bytes());
                    }
                    StreamFifoValue::U64 => {
                        let v = u64::from_ne_bytes(fifo_chunk.try_into().unwrap());
                        data[offset..offset + fifo_word_size].copy_from_slice(&v.to_ne_bytes());
                    }
                    StreamFifoValue::U128 => {
                        let v = u128::from_ne_bytes(fifo_chunk.try_into().unwrap());
                        data[offset..offset + fifo_word_size].copy_from_slice(&v.to_ne_bytes());
                    }
                    StreamFifoValue::U256 | StreamFifoValue::U512 => {
                        unimplemented!()
                    }
                }
            }
        } else {
            for n in 0..read_count {
                let offset = n * fifo_word_size;
                let v = self.axi_lite.read_u32(REG_RX_DATA)?;
                data[offset..offset + fifo_word_size].copy_from_slice(&v.to_ne_bytes());
            }
        }
        let interrupts = self.axi_lite.read_u32(REG_INTERRUPT_STATUS)?;
        if (interrupts & INTERRUPT_RX_ERROR) != 0 {
            log::warn!("Receive error, {:08x}", interrupts);
            self.reset()?;
            let error = if (interrupts & INTERRUPT_RX_OVER_READ) == INTERRUPT_RX_OVER_READ {
                Error::OverRun
            } else if (interrupts & INTERRUPT_RX_UNDER_READ) == INTERRUPT_RX_UNDER_READ
                || (interrupts & INTERRUPT_RX_UNDER_RUN) == INTERRUPT_RX_UNDER_RUN
            {
                Error::UnderRun
            } else {
                unreachable!();
            };
            return Err(error);
        }
        Ok((read_bytes, destination))
    }

    /// Writes bytes to the AXI Stream FIFO.
    pub fn write_bytes(&mut self, data: &[u8], destination: u8) -> Result<usize, Error> {
        let fifo_word_size = self.data_width.byte_count();
        let word_count = (data.len() + (fifo_word_size - 1)) / fifo_word_size;
        let mut buffer = [0u8; 64];

        self.interrupts_clear_tx()?;

        let vacancy = self.axi_lite.read_u32(REG_TX_VACANCY)? as usize;
        if vacancy < word_count {
            log::warn!(
                "Not enough vacant words, {} vacant, {} required",
                vacancy,
                word_count
            );
            return Err(Error::Full);
        }

        self.axi_lite
            .write_u32(REG_TX_DESTINATION, u32::from(destination & 0x0f))?;

        let iter = data.chunks_exact(fifo_word_size);
        let remainder = iter.remainder();

        log::debug!(
            "TX {} bytes {} words {} vacancy {} destination {} remainder",
            data.len(),
            word_count,
            vacancy,
            destination,
            remainder.len()
        );

        let num_bytes = if let Some(ref mut axi) = self.axi {
            // It seems like it is not possible to just copy slices of the same size to the FIFO data register.
            // Following type shenanigans seems to work.

            for chunk in iter.into_iter() {
                match self.data_width {
                    StreamFifoValue::U32 => {
                        axi.write_u32(
                            FULL_REG_WRITE,
                            u32::from_ne_bytes(chunk.try_into().unwrap()),
                        )?;
                    }
                    StreamFifoValue::U64 => {
                        axi.write_u64(
                            FULL_REG_WRITE,
                            u64::from_ne_bytes(chunk.try_into().unwrap()),
                        )?;
                    }
                    StreamFifoValue::U128 => {
                        axi.write_u128(
                            FULL_REG_WRITE,
                            u128::from_ne_bytes(chunk.try_into().unwrap()),
                        )?;
                    }
                    _ => {
                        unimplemented!();
                    }
                }
            }
            if remainder.len() > 0 {
                buffer[..remainder.len()].copy_from_slice(remainder);
                let part = &buffer[..fifo_word_size];
                match self.data_width {
                    StreamFifoValue::U32 => {
                        axi.write_u32(
                            FULL_REG_WRITE,
                            u32::from_ne_bytes(part.try_into().unwrap()),
                        )?;
                    }
                    StreamFifoValue::U64 => {
                        axi.write_u64(
                            FULL_REG_WRITE,
                            u64::from_ne_bytes(part.try_into().unwrap()),
                        )?;
                    }
                    StreamFifoValue::U128 => {
                        axi.write_u128(
                            FULL_REG_WRITE,
                            u128::from_ne_bytes(part.try_into().unwrap()),
                        )?;
                    }
                    _ => {
                        unimplemented!();
                    }
                }
            }
            data.len()
        } else {
            for chunk in iter {
                self.axi_lite
                    .write_u32(REG_TX_DATA, u32::from_ne_bytes(chunk.try_into().unwrap()))?;
            }
            if remainder.len() > 0 {
                buffer[..remainder.len()].copy_from_slice(remainder);
                let part = &buffer[..fifo_word_size];
                self.axi_lite
                    .write_u32(REG_TX_DATA, u32::from_ne_bytes(part.try_into().unwrap()))?;
            }
            data.len()
        };

        log::debug!("Transmit {} bytes", num_bytes);
        self.axi_lite.write_u32(REG_TX_LENGTH, num_bytes as u32)?;
        loop {
            let interrupts = self.axi_lite.read_u32(REG_INTERRUPT_STATUS)?;
            if interrupts & INTERRUPT_TX_ERROR != 0 {
                log::warn!("Transmit error, {:08x}", interrupts);
                self.reset()?;
                let error = if (interrupts & INTERRUPT_TX_OVER_RUN) == INTERRUPT_TX_OVER_RUN {
                    Error::OverRun
                } else if (interrupts & INTERRUPT_TX_LENGTH_MISMATCH)
                    == INTERRUPT_TX_LENGTH_MISMATCH
                {
                    Error::LengthMismatch
                } else {
                    unreachable!();
                };
                return Err(error);
            }
            if interrupts & INTERRUPT_TX_COMPLETE != 0 {
                break;
            }
        }
        Ok(num_bytes)
    }

    /// Writes data to the AXI Stream FIFO.
    pub fn write(&mut self, data: &[u32], destination: u8) -> Result<usize, Error> {
        let bytes = {
            let len = size_of::<u32>() * data.len();
            let ptr = data.as_ptr() as *const u8;
            unsafe {
                std::slice::from_raw_parts(ptr, len)
            }
        };
        self.write_bytes(bytes, destination)
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
/// Receive Error status interrupts
const INTERRUPT_RX_ERROR: u32 =
    INTERRUPT_RX_UNDER_RUN | INTERRUPT_RX_OVER_READ | INTERRUPT_RX_UNDER_READ;
/// Transmit Error status interrupts
const INTERRUPT_TX_ERROR: u32 = INTERRUPT_TX_OVER_RUN | INTERRUPT_TX_LENGTH_MISMATCH;
