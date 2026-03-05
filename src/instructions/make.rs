use pinocchio::cpi::Seed;
use pinocchio::error::ProgramError;
use pinocchio::{AccountView, Address, ProgramResult};
use pinocchio_token::instructions::Transfer;

use crate::instructions::helpers::{
    assert_ata_account, assert_ata_address, assert_mint_account, assert_signer, assert_system_program,
    assert_token_program, create_program_account, init_ata, parse_create_ix_data, ESCROW_SEED,
};
use crate::{Escrow, EscrowError, ID};

pub(crate) fn make(accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    let ix = parse_create_ix_data(data)?;

    let [maker, escrow, mint_a, mint_b, maker_ata_a, vault, system_program, token_program, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    assert_signer(maker)?;
    assert_system_program(system_program)?;
    assert_token_program(token_program)?;

    assert_mint_account(mint_a, token_program)?;
    assert_mint_account(mint_b, token_program)?;
    assert_ata_account(maker_ata_a, maker, mint_a, token_program)?;
    assert_ata_address(vault, escrow, mint_a, token_program)?;

    let seed_bytes = ix.seed.to_le_bytes();
    let (expected_escrow, bump) =
        Address::find_program_address(&[ESCROW_SEED, maker.address().as_ref(), &seed_bytes], &ID);

    if expected_escrow.ne(escrow.address()) {
        return Err(EscrowError::InvalidAddress.into());
    }

    let bump_bytes = [bump];
    let escrow_seeds = [
        Seed::from(ESCROW_SEED),
        Seed::from(maker.address().as_ref()),
        Seed::from(&seed_bytes),
        Seed::from(&bump_bytes),
    ];

    create_program_account(maker, escrow, &escrow_seeds, Escrow::LEN)?;
    init_ata(vault, mint_a, maker, escrow, system_program, token_program)?;

    {
        let mut escrow_data = escrow.try_borrow_mut()?;
        let escrow_state = Escrow::load_mut(escrow_data.as_mut())?;
        escrow_state.init(
            ix.seed,
            maker.address().clone(),
            mint_a.address().clone(),
            mint_b.address().clone(),
            ix.receive,
            bump,
        );
    }

    Transfer {
        from: maker_ata_a,
        to: vault,
        authority: maker,
        amount: ix.amount,
    }
    .invoke()
}
