use bitreader::{BitReader, BitReaderError};
use log::debug;
use std::collections::VecDeque;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdtsExtractorError {
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

impl From<BitReaderError> for AdtsExtractorError {
    fn from(err: BitReaderError) -> Self {
        debug!("{:?}", err);
        AdtsExtractorError::InvalidData(format!("{}", "Bad length"))
    }
}

pub type Result<T, E = AdtsExtractorError> = std::result::Result<T, E>;

trait BitReaderExpects {
    fn expect_u8(&mut self, bits: u8, expected: u8, field: &str) -> Result<()>;
}

impl BitReaderExpects for BitReader<'_> {
    fn expect_u8(&mut self, bits: u8, expected: u8, field: &str) -> Result<()> {
        let actual = self.read_u8(bits)?;
        if actual != expected {
            return Err(AdtsExtractorError::InvalidData(format!(
                "{} - expected {}, got {}",
                field, expected, actual
            )));
        }
        Ok(())
    }
}

const PACKET_LENGTH: usize = 188;

pub trait AdtsReceiver {
    fn receive(&mut self, data: &[u8]) -> Result<()>;
}

#[derive(Debug)]
enum AdtsState {
    /// Not started yet
    Sync,
    Header,
    AdaptationLength,
    Adaptation { length: u8 },
    Payload
}

pub struct AdtsExtractor<R>
where
    R: AdtsReceiver,
{
    receiver: R,
    state: AdtsState,
    buffer: VecDeque<u8>,
    pid: u16,
    pos: usize,
    has_payload: bool,
    new_payload: bool,
}

impl<R> AdtsExtractor<R>
where
    R: AdtsReceiver,
{
    fn new(receiver: R) -> AdtsExtractor<R> {
        AdtsExtractor {
            receiver,
            state: AdtsState::Sync,
            buffer: VecDeque::new(),
            pid: 0,
            pos: 0,
            has_payload: false,
            new_payload: false
        }
    }

    fn take(&mut self, count: usize) -> Option<Vec<u8>> {
        if self.buffer.len() < count {
            None
        } else {
            self.pos += count;
            Some(self.buffer.drain(..count).collect())
        }

    }

    fn reset(&mut self) {
        self.state = AdtsState::Sync;
        self.pos = 0;
        self.has_payload = false;
        self.new_payload = false;
    }

    fn read_pat(&mut self, data: Vec<u8>) -> Result<()> {

        let mut bits = BitReader::new(&data);

        let table_id = bits.read_u8(8)?;
        
        bits.expect_u8(1, 1, "section syntax indicator")?;
        bits.expect_u8(1, 0, "private bit")?;
        bits.expect_u8(2, 3, "reserved")?;
        bits.expect_u8(2, 0, "section length")?;

        let section_length = bits.read_u16(10)?;
    
        let header_size = bits.position() as usize;

        // table id extension
        bits.read_u16(2)?;

        bits.expect_u8(2, 3, "reserved")?;

        // version number
        bits.read_u8(5)?;

        // current next indicator
        bits.read_u8(1)?;

        // section number
        bits.read_u8(8)?;

        // last section number
        bits.read_u8(8)?;

        
        while bits.position() < header_size + section_length as usize {
            let program_number = bits.read_u16(16)?;
            bits.expect_u8(3, 0b111, "reserved")?;
            let program_map_pid = bits.read_u16(13)?;
            bits.expect_u8(2, 3, "reserved")?;
            if program_number == 0 {
                self.pid = program_map_pid;
            }
        }


        Ok(())
    }

    pub fn push(&mut self, data: &[u8]) -> Result<()> {
        self.buffer.extend(data);

        debug!("buffer length: {} state: {:?}", self.buffer.len(), self.state);

        match self.state {
            
            AdtsState::Sync => {
                if let Some(header) = self.take(4) {
                    self.state = AdtsState::Header;

                    let mut reader = BitReader::new(&header);

                    reader.expect_u8(8, 0x47, "sync")?;
                    reader.expect_u8(1, 0, "TEI")?;

                    // pusi
                    self.new_payload = reader.read_bool()?;

                    // transport priority
                    reader.read_u8(1)?;

                    let pid = reader.read_u16(13)?;
                    self.pid = pid;
                    debug!("PID: {}", pid);

                    reader.expect_u8(2, 0, "TSC")?;

                    let adaptation = reader.read_bool()?;
                    self.has_payload = reader.read_bool()?;

                    // continuity counter
                    reader.read_u8(4)?;

                    if adaptation {
                        self.state = AdtsState::AdaptationLength;
                    } else {
                        self.state = AdtsState::Payload;
                    }
                    self.push(&[])
                } else {
                    debug!("Not enough data for header");
                    Ok(())
                }
            }

            AdtsState::AdaptationLength => {
                if let Some(length) = self.take(1) {
                    let length = length[0];
                    self.state = AdtsState::Adaptation { length };
                    self.push(&[])
                } else {
                    debug!("Not enough data for adaptation length");
                    Ok(())
                }
            }

            AdtsState::Adaptation { length } => {
                if let Some(adaptation) = self.take(length as usize) {
                    // don't think we need anything here

                    if self.has_payload {
                        self.state = AdtsState::Payload;
                    } else {
                        self.reset();
                    }
                    self.push(&adaptation)
                } else {
                    debug!("Not enough data for adaptation");
                    Ok(())
                }
            }

            AdtsState::Payload => {
                if let Some(mut payload) = self.take(PACKET_LENGTH - self.pos) {
                    // self.receiver.receive(&payload)?;
                    debug!("Payload length: {:?}", payload.len());

                    let payload = if self.new_payload {
                        let pointer = payload[0];
                        // TODO handle continuation
                        payload.split_off(pointer as usize)
                    } else {
                        payload
                    };

                    match self.pid {
                        
                        0 => self.read_pat(payload)?,

                        _ => {
                            debug!("Unknown PID: {}", self.pid);
                        }
                    }

                    self.reset();
                    self.push(&[])
                } else {
                    debug!("Not enough data for payload");
                    Ok(())
                }
            }

            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    fn it_works() {
        env_logger::try_init().ok();

        struct DummyReceiver {}

        impl AdtsReceiver for DummyReceiver {
            fn receive(&mut self, data: &[u8]) -> Result<()> {
                println!("{:?}", data);
                Ok(())
            }
        }

        let mut extractor = AdtsExtractor::new(DummyReceiver {});

        let mut f = std::fs::File::open("test.ts").unwrap();
        loop {
            let mut buf = [0u8; 1024];
            let n = f.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            extractor.push(&mut buf.to_vec()).unwrap();
        }
    }
}
