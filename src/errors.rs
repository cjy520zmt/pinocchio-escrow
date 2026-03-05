use pinocchio::error::ProgramError;

#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowError {
    MissingRequiredSignature = 0,
    InvalidProgram = 1,
    InvalidOwner = 2,
    InvalidAccountData = 3,
    InvalidInstruction = 4,
    InvalidAmount = 5,
    InvalidAddress = 6,
    InvalidEscrowState = 7,
}

impl From<EscrowError> for ProgramError {
    fn from(value: EscrowError) -> Self {
        ProgramError::Custom(value as u32)
    }
}
