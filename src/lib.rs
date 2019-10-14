///https://github.com/Microsoft/uf2/blob/master/hf2.md
use core::convert::TryFrom;
use scroll::{ctx, Pread, Pwrite, LE};
mod mock;
use mock::HidMockable;

#[derive(Debug, PartialEq)]
pub enum BinInfoMode {
    //bootloader, and thus flashing of user-space programs is allowed
    Bootloader = 0x0001,
    //user-space mode. It also returns the size of flash page size (flashing needs to be done on page-by-page basis), and the maximum size of message. It is always the case that max_message_size >= flash_page_size + 64.
    User = 0x0002,
}

impl TryFrom<u32> for BinInfoMode {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(BinInfoMode::Bootloader),
            2 => Ok(BinInfoMode::User),
            _ => Err(Error::Parse),
        }
    }
}

/// This command states the current mode of the device:
pub struct BinInfo {}

impl Commander<BinInfoResult> for BinInfo {
    fn send(&self, d: &hidapi::HidDevice) -> Result<BinInfoResult, Error> {
        let command = Command::new(0x0001, 0, vec![]);

        xmit(command, d)?;

        let rsp = rx(d)?;

        if rsp.status != CommandResponseStatus::Success {
            return Err(Error::MalformedRequest);
        }

        let res: BinInfoResult = (rsp.data.as_slice()).pread_with::<BinInfoResult>(0, LE)?;

        Ok(res)
    }
}

#[derive(Debug, PartialEq)]
pub struct BinInfoResult {
    pub mode: BinInfoMode, //    uint32_t mode;
    pub flash_page_size: u32,
    pub flash_num_pages: u32,
    pub max_message_size: u32,
    pub family_id: FamilyId, // optional?
}

#[allow(non_camel_case_types)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FamilyId {
    ATSAMD21,
    ATSAMD51,
    NRF52840,
    STM32F103,
    STM32F401,
    ATMEGA32,
    CYPRESS_FX2,
    UNKNOWN(u32),
}

impl From<u32> for FamilyId {
    fn from(val: u32) -> Self {
        match val {
            0x68ed_2b88 => Self::ATSAMD21,
            0x5511_4460 => Self::ATSAMD51,
            0x1b57_745f => Self::NRF52840,
            0x5ee2_1072 => Self::STM32F103,
            0x5775_5a57 => Self::STM32F401,
            0x1657_3617 => Self::ATMEGA32,
            0x5a18_069b => Self::CYPRESS_FX2,
            _ => Self::UNKNOWN(val),
        }
    }
}

impl CommanderResult for BinInfoResult {}

impl<'a> ctx::TryFromCtx<'a, scroll::Endian> for BinInfoResult {
    type Error = Error;
    fn try_from_ctx(this: &'a [u8], le: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        if this.len() < 16 {
            return Err(Error::Parse);
        }

        //does it give me offset somehow??? or just slice appropriately for me?s
        let mut offset = 0;
        let mode: u32 = this.gread_with::<u32>(&mut offset, le)?;
        let mode: Result<BinInfoMode, Error> = BinInfoMode::try_from(mode);
        let mode: BinInfoMode = mode?;
        let flash_page_size = this.gread_with::<u32>(&mut offset, le)?;
        let flash_num_pages = this.gread_with::<u32>(&mut offset, le)?;
        let max_message_size = this.gread_with::<u32>(&mut offset, le)?;

        //todo, not sure if optional means it would be 0, or would not be included at all
        let family_id: FamilyId = this.gread_with::<u32>(&mut offset, le)?.into();

        Ok((
            BinInfoResult {
                mode,
                flash_page_size,
                flash_num_pages,
                max_message_size,
                family_id,
            },
            offset,
        ))
    }
}

/// Various device information. The result is a character array. See INFO_UF2.TXT in UF2 format for details.
pub struct Info {}
impl Commander<InfoResult> for Info {
    fn send(&self, d: &hidapi::HidDevice) -> Result<InfoResult, Error> {
        let command = Command::new(0x0002, 0, vec![]);

        xmit(command, d)?;

        let rsp = rx(d)?;

        if rsp.status != CommandResponseStatus::Success {
            return Err(Error::MalformedRequest);
        }

        let res: InfoResult = (rsp.data.as_slice()).pread_with::<InfoResult>(0, LE)?;

        Ok(res)
    }
}

#[derive(Debug, PartialEq)]
pub struct InfoResult {
    pub info: String,
}
impl CommanderResult for InfoResult {}

impl<'a> ctx::TryFromCtx<'a, scroll::Endian> for InfoResult {
    type Error = Error;
    fn try_from_ctx(this: &'a [u8], le: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        let mut bytes = vec![0; this.len()];

        let mut offset = 0;
        this.gread_inout_with(&mut offset, &mut bytes, le)?;

        let info = std::str::from_utf8(&bytes)?;

        Ok((InfoResult { info: info.into() }, offset))
    }
}

///Write a single page of flash memory. No Result.
pub struct WriteFlashPage {
    pub target_address: u32,
    pub data: Vec<u8>,
}

impl Commander<NoResult> for WriteFlashPage {
    fn send(&self, d: &hidapi::HidDevice) -> Result<NoResult, Error> {
        let mut data = vec![0_u8; self.data.len() + 4];

        let mut offset = 0;

        data.gwrite_with(self.target_address, &mut offset, LE)?;

        for i in &self.data {
            data.gwrite_with(i, &mut offset, LE)?;
        }

        let command = Command::new(0x0006, 0, data);

        xmit(command, d)?;

        let _ = rx(d)?;

        Ok(NoResult {})
    }
}

///Compute checksum of a number of pages. Maximum value for num_pages is max_message_size / 2 - 2. The checksum algorithm used is CRC-16-CCITT.
pub struct ChksumPages {
    pub target_address: u32,
    pub num_pages: u32,
}

impl Commander<ChksumPagesResult> for ChksumPages {
    fn send(&self, d: &hidapi::HidDevice) -> Result<ChksumPagesResult, Error> {
        let data = &mut [0_u8; 8];

        let mut offset = 0;

        data.gwrite_with(self.target_address, &mut offset, LE)?;
        data.gwrite_with(self.num_pages, &mut offset, LE)?;

        let command = Command::new(0x0007, 0, data.to_vec());

        xmit(command, d)?;

        let rsp = rx(d)?;

        if rsp.status != CommandResponseStatus::Success {
            return Err(Error::MalformedRequest);
        }

        let res: ChksumPagesResult =
            (rsp.data.as_slice()).pread_with::<ChksumPagesResult>(0, LE)?;

        Ok(res)
    }
}

///Maximum value for num_pages is max_message_size / 2 - 2. The checksum algorithm used is CRC-16-CCITT.
#[derive(Debug, PartialEq)]
pub struct ChksumPagesResult {
    pub chksums: Vec<u16>,
}
impl CommanderResult for ChksumPagesResult {}

impl<'a> ctx::TryFromCtx<'a, scroll::Endian> for ChksumPagesResult {
    type Error = Error;
    fn try_from_ctx(this: &'a [u8], le: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        let mut chksums: Vec<u16> = vec![0; this.len() / 2];

        let mut offset = 0;
        this.gread_inout_with(&mut offset, &mut chksums, le)?;

        Ok((ChksumPagesResult { chksums }, offset))
    }
}

///Reset the device into user-space app. Usually, no response at all will arrive for this command.
pub struct ResetIntoApp {}
impl Commander<NoResult> for ResetIntoApp {
    fn send(&self, d: &hidapi::HidDevice) -> Result<NoResult, Error> {
        let command = Command::new(0x0003, 0, vec![]);

        xmit(command, d)?;

        Ok(NoResult {})
    }
}

///Reset the device into bootloader, usually for flashing. Usually, no response at all will arrive for this command.
pub struct ResetIntoBootloader {}
impl Commander<NoResult> for ResetIntoBootloader {
    fn send(&self, d: &hidapi::HidDevice) -> Result<NoResult, Error> {
        let command = Command::new(0x0004, 0, vec![]);

        xmit(command, d)?;

        Ok(NoResult {})
    }
}

/// When issued in bootloader mode, it has no effect. In user-space mode it causes handover to bootloader. A BININFO command can be issued to verify that.
pub struct StartFlash {}
impl Commander<NoResult> for StartFlash {
    fn send(&self, d: &hidapi::HidDevice) -> Result<NoResult, Error> {
        let command = Command::new(0x0005, 0, vec![]);

        xmit(command, d)?;

        let _ = rx(d)?;

        Ok(NoResult {})
    }
}

///Read a number of words from memory. Memory is read word by word (and not byte by byte), and target_addr must be suitably aligned. This is to support reading of special IO regions.
pub struct ReadWords {
    target_address: u32,
    num_words: u32,
}

impl Commander<ReadWordsResult> for ReadWords {
    fn send(&self, d: &hidapi::HidDevice) -> Result<ReadWordsResult, Error> {
        let data = &mut [0_u8; 8];

        let mut offset = 0;

        data.gwrite_with(self.target_address, &mut offset, LE)?;
        data.gwrite_with(self.num_words, &mut offset, LE)?;

        let command = Command::new(0x0008, 0, data.to_vec());

        xmit(command, d)?;

        let rsp = rx(d)?;

        if rsp.status != CommandResponseStatus::Success {
            return Err(Error::MalformedRequest);
        }

        let res: ReadWordsResult = (rsp.data.as_slice()).pread_with::<ReadWordsResult>(0, LE)?;

        Ok(res)
    }
}

#[derive(Debug, PartialEq)]
pub struct ReadWordsResult {
    pub words: Vec<u32>,
}
impl CommanderResult for ReadWordsResult {}

impl<'a> ctx::TryFromCtx<'a, scroll::Endian> for ReadWordsResult {
    type Error = Error;
    fn try_from_ctx(this: &'a [u8], le: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        let mut words: Vec<u32> = vec![0; this.len() / 4];

        let mut offset = 0;
        this.gread_inout_with(&mut offset, &mut words, le)?;

        Ok((ReadWordsResult { words }, offset))
    }
}

///Dual of READ WORDS, with the same constraints. No Result.
pub struct WriteWords {
    pub target_address: u32,
    pub num_words: u32,
    pub words: Vec<u32>,
}

impl Commander<NoResult> for WriteWords {
    fn send(&self, d: &hidapi::HidDevice) -> Result<NoResult, Error> {
        let data = &mut [0_u8; 64];

        let mut offset = 0;

        data.gwrite_with(self.target_address, &mut offset, LE)?;
        data.gwrite_with(self.num_words, &mut offset, LE)?;

        for i in &self.words {
            data.gwrite_with(i, &mut offset, LE)?;
        }

        let command = Command::new(0x0009, 0, data.to_vec());

        xmit(command, d)?;

        let _ = rx(d)?;

        Ok(NoResult {})
    }
}

///Return internal log buffer if any. The result is a character array.
pub struct Dmesg {}

impl Commander<DmesgResult> for Dmesg {
    fn send(&self, d: &hidapi::HidDevice) -> Result<DmesgResult, Error> {
        let command = Command::new(0x0010, 0, vec![]);

        xmit(command, d)?;

        let rsp = rx(d)?;

        if rsp.status != CommandResponseStatus::Success {
            return Err(Error::MalformedRequest);
        }

        let res: DmesgResult = (rsp.data.as_slice()).pread_with::<DmesgResult>(0, LE)?;

        Ok(res)
    }
}

#[derive(Debug, PartialEq)]
pub struct DmesgResult {
    pub logs: String,
}
impl CommanderResult for DmesgResult {}

impl<'a> ctx::TryFromCtx<'a, scroll::Endian> for DmesgResult {
    type Error = Error;
    fn try_from_ctx(this: &'a [u8], le: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        let mut bytes = vec![0; this.len()];

        let mut offset = 0;
        this.gread_inout_with(&mut offset, &mut bytes, le)?;

        let logs = std::str::from_utf8(&bytes)?;

        Ok((DmesgResult { logs: logs.into() }, offset))
    }
}

pub trait CommanderResult {}

pub trait Commander<RES: CommanderResult> {
    fn send(&self, d: &hidapi::HidDevice) -> Result<RES, Error>;
}

pub struct NoResult {}
impl CommanderResult for NoResult {}

#[derive(Debug, PartialEq)]
struct CommandResponse {
    ///arbitrary number set by the host, for example as sequence number. The response should repeat the tag.
    tag: u16,
    status: CommandResponseStatus, //    uint8_t status;
    ///additional information In case of non-zero status
    status_info: u8, // optional?
    ///LE bytes
    data: Vec<u8>,
}

#[derive(Debug, PartialEq)]
enum CommandResponseStatus {
    //command understood and executed correctly
    Success = 0x00,
    //command not understood
    ParseError = 0x01,
    //command execution error
    ExecutionError = 0x02,
}

impl TryFrom<u8> for CommandResponseStatus {
    type Error = Error;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(CommandResponseStatus::Success),
            1 => Ok(CommandResponseStatus::ParseError),
            2 => Ok(CommandResponseStatus::ExecutionError),
            _ => Err(Error::Parse),
        }
    }
}

#[derive(Debug, PartialEq)]
enum PacketType {
    //Inner packet of a command message
    Inner = 0,
    //Final packet of a command message
    Final = 1,
    //Serial stdout
    StdOut = 2,
    //Serial stderr
    Stderr = 3,
}

impl TryFrom<u8> for PacketType {
    type Error = Error;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(PacketType::Inner),
            1 => Ok(PacketType::Final),
            2 => Ok(PacketType::StdOut),
            3 => Ok(PacketType::Stderr),
            _ => Err(Error::Parse),
        }
    }
}

// doesnt know what the data is supposed to be decoded as
// thats linked via the seq number outside, so we cant decode here
impl<'a> ctx::TryFromCtx<'a, scroll::Endian> for CommandResponse {
    type Error = Error;
    fn try_from_ctx(this: &'a [u8], le: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        if this.len() < 4 {
            return Err(Error::Parse);
        }

        let mut offset = 0;
        let tag = this.gread_with::<u16>(&mut offset, le)?;
        let status: u8 = this.gread_with::<u8>(&mut offset, le)?;
        let status = CommandResponseStatus::try_from(status)?;
        let status_info = this.gread_with::<u8>(&mut offset, le)?;

        Ok((
            CommandResponse {
                tag,
                status,
                status_info,
                data: this[offset..].to_vec(),
            },
            offset,
        ))
    }
}

struct Command {
    ///Command ID
    id: u32,
    ///arbitrary number set by the host, for example as sequence number. The response should repeat the tag.
    tag: u16,
    ///reserved bytes in the command should be sent as zero and ignored by the device
    _reserved0: u8,
    ///reserved bytes in the command should be sent as zero and ignored by the device
    _reserved1: u8,
    ///LE bytes
    data: Vec<u8>,
}
impl Command {
    fn new(id: u32, tag: u16, data: Vec<u8>) -> Self {
        Self {
            id,
            tag,
            _reserved0: 0,
            _reserved1: 0,
            data,
        }
    }
}

///Transmit a Command, command.data should already have been LE converted
fn xmit<T: HidMockable>(cmd: Command, d: &T) -> Result<(), Error> {
    //Packets are up to 64 bytes long
    let buffer = &mut [0_u8; 64];

    // header is 1
    let mut offset = 1;

    //command struct is 8 bytes
    buffer.gwrite_with(cmd.id, &mut offset, LE)?;
    buffer.gwrite_with(cmd.tag, &mut offset, LE)?;
    buffer.gwrite_with(cmd._reserved0, &mut offset, LE)?;
    buffer.gwrite_with(cmd._reserved1, &mut offset, LE)?;

    let mut count = if cmd.data.len() > 55 {
        55
    } else {
        cmd.data.len()
    };

    //send up to the first 55 bytes
    for (i, val) in cmd.data[..count].iter().enumerate() {
        buffer[i + offset] = *val
    }

    if count == cmd.data.len() {
        buffer[0] = (PacketType::Final as u8) << 6 | (8 + count) as u8;
        d.my_write(buffer)?;
        return Ok(());
    } else {
        buffer[0] = (PacketType::Inner as u8) << 6 | (8 + count) as u8;
        d.my_write(buffer)?;
    }

    //send the rest in chunks up to 63
    for chunk in cmd.data[count..].chunks(64 - 1 as usize) {
        count += chunk.len();

        if count == cmd.data.len() {
            buffer[0] = (PacketType::Final as u8) << 6 | chunk.len() as u8;
        } else {
            buffer[0] = (PacketType::Inner as u8) << 6 | chunk.len() as u8;
        }

        for (i, val) in chunk.iter().enumerate() {
            buffer[i + 1] = *val
        }

        // println!("tx: {:02X?}", &buffer[..=chunk.len()));

        d.my_write(&buffer[..=chunk.len()])?;
    }
    Ok(())
}

///Receive a CommandResponse, CommandResponse.data is not interpreted in any way.
fn rx<T: HidMockable>(d: &T) -> Result<CommandResponse, Error> {
    let mut bitsnbytes: Vec<u8> = vec![];

    let buffer = &mut [0_u8; 64];

    // keep reading until Final packet
    while {
        d.my_read(buffer)?;

        let ptype = PacketType::try_from(buffer[0] >> 6)?;

        let len: usize = (buffer[0] & 0x3F) as usize;
        // println!(
        //     "rx header: {:02X?} (ptype: {:?} len: {:?}) data: {:02X?}",
        //     &buffer[0],
        //     ptype,
        //     len,
        //     &buffer[1..=len]
        // );

        //skip the header byte and strip excess bytes remote is allowed to send
        bitsnbytes.extend_from_slice(&buffer[1..=len]);

        ptype == PacketType::Inner
    } {}

    let resp = bitsnbytes.as_slice().pread_with::<CommandResponse>(0, LE)?;

    Ok(resp)
}

#[derive(Clone, Debug)]
pub enum Error {
    Arguments,
    Parse,
    MalformedRequest,
    Execution,
    Sequence,
    Transmission,
}

impl From<hidapi::HidError> for Error {
    fn from(_err: hidapi::HidError) -> Self {
        Error::Transmission
    }
}

impl From<scroll::Error> for Error {
    fn from(_err: scroll::Error) -> Self {
        Error::Parse
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(_err: std::str::Utf8Error) -> Self {
        Error::Parse
    }
}

impl From<std::io::Error> for Error {
    fn from(_err: std::io::Error) -> Self {
        Error::Arguments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mock::MyMock;

    #[test]
    fn send_fragmented() {
        let data: Vec<Vec<u8>> = vec![
            vec![
                0x3f, 0x06, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
                0x00, 0x03, 0x20, 0xd7, 0x5e, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x51, 0x5f, 0x00,
                0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00,
            ],
            vec![
                0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00,
                0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f,
                0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00,
                0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f,
                0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f,
            ],
            vec![
                0x3f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00,
                0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d,
                0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00,
                0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d,
                0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d,
            ],
            vec![
                0x3f, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f,
                0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00,
                0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f,
                0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
            vec![
                0x50, 0x00, 0x00, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d, 0x5f, 0x00, 0x00, 0x4d,
                0x5f, 0x00, 0x00,
            ],
        ];

        let le_page: Vec<u8> = vec![
            0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x03, 0x20, 0xD7, 0x5E, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x51, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F,
            0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
            0x4D, 0x5F, 0x00, 0x00, 0x4D, 0x5F, 0x00, 0x00,
        ];

        let writer = |v: &[u8]| -> usize {
            static mut I: usize = 0;

            let res: &Vec<u8> = unsafe {
                let res = &data[I];
                I += 1;
                res
            };

            assert_eq!(res.as_slice(), v);

            v.len()
        };

        let mock = MyMock {
            reader: || vec![],
            writer,
        };

        let command = Command::new(0x0006, 4, le_page);

        xmit(command, &mock).unwrap();
    }

    #[test]
    fn receive_fragmented() {
        let data: Vec<Vec<u8>> = vec![
            vec![
                0x3F, 0x04, 0x00, 0x00, 0x00, 0x55, 0x46, 0x32, 0x20, 0x42, 0x6F, 0x6F, 0x74, 0x6C,
                0x6F, 0x61, 0x64, 0x65, 0x72, 0x20, 0x76, 0x33, 0x2E, 0x36, 0x2E, 0x30, 0x20, 0x53,
                0x46, 0x48, 0x57, 0x52, 0x4F, 0x0D, 0x0A, 0x4D, 0x6F, 0x64, 0x65, 0x6C, 0x3A, 0x20,
                0x50, 0x79, 0x47, 0x61, 0x6D, 0x65, 0x72, 0x0D, 0x0A, 0x42, 0x6F, 0x61, 0x72, 0x64,
                0x2D, 0x49, 0x44, 0x3A, 0x20, 0x53, 0x41, 0x4D,
            ],
            vec![
                0x54, 0x44, 0x35, 0x31, 0x4A, 0x31, 0x39, 0x41, 0x2D, 0x50, 0x79, 0x47, 0x61, 0x6D,
                0x65, 0x72, 0x2D, 0x4D, 0x34, 0x0D, 0x0A,
            ],
        ];

        let result: Vec<u8> = vec![
            0x55, 0x46, 0x32, 0x20, 0x42, 0x6F, 0x6F, 0x74, 0x6C, 0x6F, 0x61, 0x64, 0x65, 0x72,
            0x20, 0x76, 0x33, 0x2E, 0x36, 0x2E, 0x30, 0x20, 0x53, 0x46, 0x48, 0x57, 0x52, 0x4F,
            0x0D, 0x0A, 0x4D, 0x6F, 0x64, 0x65, 0x6C, 0x3A, 0x20, 0x50, 0x79, 0x47, 0x61, 0x6D,
            0x65, 0x72, 0x0D, 0x0A, 0x42, 0x6F, 0x61, 0x72, 0x64, 0x2D, 0x49, 0x44, 0x3A, 0x20,
            0x53, 0x41, 0x4D, 0x44, 0x35, 0x31, 0x4A, 0x31, 0x39, 0x41, 0x2D, 0x50, 0x79, 0x47,
            0x61, 0x6D, 0x65, 0x72, 0x2D, 0x4D, 0x34, 0x0D, 0x0A,
        ];

        let reader = || -> Vec<u8> {
            static mut I: usize = 0;

            let res: &Vec<u8> = unsafe {
                let res = &data[I];
                I += 1;
                res
            };

            res.to_vec()
        };

        let mock = MyMock {
            reader,
            writer: |_v| 0,
        };

        let response = CommandResponse {
            tag: 0x0004,
            status: CommandResponseStatus::Success,
            status_info: 0x00,
            data: result.to_vec(),
        };

        let rsp = rx(&mock).unwrap();
        assert_eq!(rsp, response);
    }

    #[test]
    fn parse_response() {
        let data: Vec<u8> = vec![
            0x55, 0x46, 0x32, 0x20, 0x42, 0x6F, 0x6F, 0x74, 0x6C, 0x6F, 0x61, 0x64, 0x65, 0x72,
            0x20, 0x76, 0x33, 0x2E, 0x36, 0x2E, 0x30, 0x20, 0x53, 0x46, 0x48, 0x57, 0x52, 0x4F,
            0x0D, 0x0A, 0x4D, 0x6F, 0x64, 0x65, 0x6C, 0x3A, 0x20, 0x50, 0x79, 0x47, 0x61, 0x6D,
            0x65, 0x72, 0x0D, 0x0A, 0x42, 0x6F, 0x61, 0x72, 0x64, 0x2D, 0x49, 0x44, 0x3A, 0x20,
            0x53, 0x41, 0x4D, 0x44, 0x35, 0x31, 0x4A, 0x31, 0x39, 0x41, 0x2D, 0x50, 0x79, 0x47,
            0x61, 0x6D, 0x65, 0x72, 0x2D, 0x4D, 0x34, 0x0D, 0x0A,
        ];

        let info_result = InfoResult {
info: "UF2 Bootloader v3.6.0 SFHWRO\r\nModel: PyGamer\r\nBoard-ID: SAMD51J19A-PyGamer-M4\r\n".into()
        };

        let res: InfoResult = (data.as_slice()).pread_with::<InfoResult>(0, LE).unwrap();

        assert_eq!(res, info_result);
    }
}