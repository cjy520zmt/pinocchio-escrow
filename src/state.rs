use core::mem::size_of;

use pinocchio::error::ProgramError;
use pinocchio::Address;

use crate::EscrowError;

// 固定二进制布局的托管状态。前端会按同样的 offset 解码该账户。
#[repr(C)]
pub struct Escrow {
    // maker 自定义随机种子（参与 escrow PDA 推导）。
    pub seed: u64,
    // 创建托管者地址。
    pub maker: Address,
    // maker 存入的资产 mint。
    pub mint_a: Address,
    // taker 需要支付给 maker 的资产 mint。
    pub mint_b: Address,
    // taker 需要支付的 mint_b 数量。
    pub receive: u64,
    // escrow PDA bump（用于后续签名）。
    pub bump: [u8; 1],
}

impl Escrow {
    // 明确账户大小，避免由编译器 padding 或推断导致偏差。
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
        // 长度已校验，按固定布局零拷贝视图读取。
        Ok(unsafe { &*core::mem::transmute::<*const u8, *const Self>(bytes.as_ptr()) })
    }

    #[inline(always)]
    pub fn load_mut(bytes: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if bytes.len() != Self::LEN {
            return Err(EscrowError::InvalidAccountData.into());
        }
        // 长度已校验，按固定布局零拷贝视图写入。
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

// 指令处理中只需要的轻量字段，避免暴露/复制完整可变状态。
#[derive(Clone, Copy)]
pub(crate) struct EscrowSnapshot {
    pub(crate) seed: u64,
    pub(crate) receive: u64,
    pub(crate) bump: [u8; 1],
}
