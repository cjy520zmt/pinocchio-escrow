use pinocchio::error::ProgramError;

// `repr(u32)` 保证错误码稳定，便于客户端按 custom error code 映射提示文案。
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

// 将业务错误转换成 Solana runtime 可识别的 ProgramError::Custom(code)。
impl From<EscrowError> for ProgramError {
    fn from(value: EscrowError) -> Self {
        ProgramError::Custom(value as u32)
    }
}
