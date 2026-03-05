use pinocchio::cpi::{Seed, Signer};
use pinocchio::error::ProgramError;
use pinocchio::{AccountView, ProgramResult};
use pinocchio_token::instructions::{CloseAccount, Transfer};

use crate::instructions::helpers::{
    assert_ata_account, assert_mint_account, assert_program_account, assert_signer, assert_system_program,
    assert_token_program, close_program_account, init_ata_if_needed, load_and_validate_escrow,
    token_account_amount, ESCROW_SEED,
};
use crate::{Escrow, ID};

pub(crate) fn refund(accounts: &[AccountView]) -> ProgramResult {
    // 账户顺序必须与客户端 `buildRefundTransaction` 的 keys 一致。
    let [maker, escrow, mint_a, vault, maker_ata_a, system_program, token_program, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    assert_signer(maker)?;
    assert_system_program(system_program)?;
    assert_token_program(token_program)?;

    assert_program_account(escrow, Escrow::LEN, &ID)?;
    assert_mint_account(mint_a, token_program)?;
    assert_ata_account(vault, escrow, mint_a, token_program)?;

    // 仅验证 maker + mint_a。refund 不涉及 mint_b。
    let escrow_state = load_and_validate_escrow(escrow, maker, mint_a, None)?;

    // maker 的 mint_a ATA 不存在时自动创建。
    init_ata_if_needed(
        maker_ata_a,
        mint_a,
        maker,
        maker,
        system_program,
        token_program,
    )?;

    let vault_amount = token_account_amount(vault)?;

    // escrow PDA 签名，授权 vault -> maker_ata_a 回退。
    let seed_bytes = escrow_state.seed.to_le_bytes();
    let bump_bytes = escrow_state.bump;
    let escrow_seeds = [
        Seed::from(ESCROW_SEED),
        Seed::from(maker.address().as_ref()),
        Seed::from(&seed_bytes),
        Seed::from(&bump_bytes),
    ];
    let signer = Signer::from(&escrow_seeds);

    Transfer {
        from: vault,
        to: maker_ata_a,
        authority: escrow,
        amount: vault_amount,
    }
    .invoke_signed(&[signer.clone()])?;

    CloseAccount {
        account: vault,
        destination: maker,
        authority: escrow,
    }
    .invoke_signed(&[signer])?;

    // 退款后清理 escrow 账户，返还租金给 maker。
    close_program_account(escrow, maker)
}
