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

pub(crate) fn take(accounts: &[AccountView]) -> ProgramResult {
    // 账户顺序必须与客户端 `buildTakeTransaction` 的 keys 一致。
    let [taker, maker, escrow, mint_a, mint_b, vault, taker_ata_a, taker_ata_b, maker_ata_b, system_program, token_program, ..] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    assert_signer(taker)?;
    assert_system_program(system_program)?;
    assert_token_program(token_program)?;

    assert_program_account(escrow, Escrow::LEN, &ID)?;
    assert_mint_account(mint_a, token_program)?;
    assert_mint_account(mint_b, token_program)?;
    assert_ata_account(vault, escrow, mint_a, token_program)?;
    assert_ata_account(taker_ata_b, taker, mint_b, token_program)?;

    // 读取并校验 escrow 内的 maker/mint_a/mint_b 与传入账户是否一致。
    let escrow_state = load_and_validate_escrow(escrow, maker, mint_a, Some(mint_b))?;

    // 按需创建收款 ATA（若不存在）；存在则做 owner/len 校验。
    init_ata_if_needed(
        taker_ata_a,
        mint_a,
        taker,
        taker,
        system_program,
        token_program,
    )?;
    init_ata_if_needed(
        maker_ata_b,
        mint_b,
        taker,
        maker,
        system_program,
        token_program,
    )?;

    let vault_amount = token_account_amount(vault)?;

    // 用 escrow PDA（含 bump）签名，授权从 vault 转账与关闭 vault。
    let seed_bytes = escrow_state.seed.to_le_bytes();
    let bump_bytes = escrow_state.bump;
    let escrow_seeds = [
        Seed::from(ESCROW_SEED),
        Seed::from(maker.address().as_ref()),
        Seed::from(&seed_bytes),
        Seed::from(&bump_bytes),
    ];
    let signer = Signer::from(&escrow_seeds);

    // 先执行 taker -> maker 的支付腿；失败时 vault 资金不会释放。
    Transfer {
        from: taker_ata_b,
        to: maker_ata_b,
        authority: taker,
        amount: escrow_state.receive,
    }
    .invoke()?;

    Transfer {
        from: vault,
        to: taker_ata_a,
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

    // 交易完成后关闭 escrow 账户，把租金返还给 maker。
    close_program_account(escrow, maker)
}
