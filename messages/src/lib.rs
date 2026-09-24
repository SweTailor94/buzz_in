#![no_std]
#![no_main]

use core::convert::Infallible;
use serde::{Deserialize, Serialize};
use postcard2::{from_bytes, BufferFull, DeserializerError, SerializerError, UnexpectedEnd};
use postcard2::to_slice;
use heapless::Vec;

#[derive(Serialize,Deserialize,Debug, Clone)]
pub enum Command {
    Buttons(
        [ButtonState; 5]),
    Reset,
    None,
    PollButtonState,
}

impl Command {
     pub fn as_str(&self) -> &'static str {
         match self {
             Command::Buttons(_) => "Buttons",
             Command::Reset => "Reset",
             Command::None => "None",
             Command::PollButtonState => "PollButtonState",
         }
     }
}

#[derive(Serialize,Deserialize,Debug, Clone, Copy)]
pub enum ButtonState{
    Red,
    Green,
    Blue,
    Yellow,
    White,
    None,
}

impl Default for ButtonState {
    fn default() -> Self {
        ButtonState::None
    }
}

impl Default for Command {
    fn default() -> Self {
        Command::None
    }
}

impl Command {

    // pub fn to_vec(&self) -> Result<Vec<u8, 32>, SerializerError> {
    //     match  to_vec::<Command,32>(&self) {
    //         Ok(data) => {
    //             Ok(data)
    //         }
    //         Err(e) => {
    //             Err(e)
    //         }
    //     }
    // }

    pub fn to_vec(&self) -> Result<Vec<u8, 32> , SerializerError<BufferFull, Infallible>> {
        let mut buf = [0u8; 32];
        let slice = to_slice(&self, &mut buf)?;
        Vec::from_slice(slice.as_ref()).map_err(|_| SerializerError::PushError(BufferFull))
    }

    pub fn to_slice(&self, buf: &mut [u8] ) ->  Result<(), ()>{
        to_slice(self, buf).map_err(|_| ())?;
        Ok(())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Command, DeserializerError<UnexpectedEnd,Infallible>> {
        from_bytes(bytes)
    }
}