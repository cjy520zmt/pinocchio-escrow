use pinocchio::{entrypoint, AccountView, Address, ProgramResult};

mod errors;
mod instructions;
mod state;

pub use errors::EscrowError;
pub use state::Escrow;

use crate::instructions::{make, refund, take};

entrypoint!(process_instruction);

// 22222222222222222222222222222222222222222222
pub const ID: Address = Address::new_from_array([
    0x0f, 0x1e, 0x6b, 0x14, 0x21, 0xc0, 0x4a, 0x07, 0x04, 0x31, 0x26, 0x5c, 0x19, 0xc5, 0xbb, 0xee,
    0x19, 0x92, 0xba, 0xe8, 0xaf, 0xd1, 0xcd, 0x07, 0x8e, 0xf8, 0xaf, 0x70, 0x47, 0xdc, 0x11, 0xf7,
]);

fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    if program_id.ne(&ID) {
        return Err(EscrowError::InvalidProgram.into());
    }

    let (discriminator, data) = instruction_data
        .split_first()
        .ok_or(EscrowError::InvalidInstruction)?;

    match *discriminator {
        0 => make(accounts, data),
        1 => {
            if !data.is_empty() {
                return Err(EscrowError::InvalidInstruction.into());
            }
            take(accounts)
        }
        2 => {
            if !data.is_empty() {
                return Err(EscrowError::InvalidInstruction.into());
            }
            refund(accounts)
        }
        _ => Err(EscrowError::InvalidInstruction.into()),
    }
}
