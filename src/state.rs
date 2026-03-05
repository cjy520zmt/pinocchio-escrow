use core::mem::size_of;

use pinocchio::error::ProgramError;
use pinocchio::Address;

use crate::EscrowError;

#[repr(C)]
pub struct Escrow {
    pub seed: u64,
    pub maker: Address,
    pub mint_a: Address,
    pub mint_b: Address,
    pub receive: u64,
    pub bump: [u8; 1],
}

impl Escrow {
    pub const LEN: usize = size_of::<u64>()
        + size_of::<Address>()
        + size_of::<Address>()
        + size_of::<Address>()
        + size_of::<u64>()
        + size_of::<[u8; 1]>();

    #[inline(always)]
    pub fn load(bytes: &[u8]) -> Result<&Self, ProgramError> {
        if bytes.len() != Self::LEN {
            return Err(EscrowError::InvalidAccountData.into());
        }
        Ok(unsafe { &*core::mem::transmute::<*const u8, *const Self>(bytes.as_ptr()) })
    }

    #[inline(always)]
    pub fn load_mut(bytes: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if bytes.len() != Self::LEN {
            return Err(EscrowError::InvalidAccountData.into());
        }
        Ok(unsafe { &mut *core::mem::transmute::<*mut u8, *mut Self>(bytes.as_mut_ptr()) })
    }

    #[inline(always)]
    pub fn init(
        &mut self,
        seed: u64,
        maker: Address,
        mint_a: Address,
        mint_b: Address,
        receive: u64,
        bump: u8,
    ) {
        self.seed = seed;
        self.maker = maker;
        self.mint_a = mint_a;
        self.mint_b = mint_b;
        self.receive = receive;
        self.bump = [bump];
    }
}

#[derive(Clone, Copy)]
pub(crate) struct EscrowSnapshot {
    pub(crate) seed: u64,
    pub(crate) receive: u64,
    pub(crate) bump: [u8; 1],
}
