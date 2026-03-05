use core::mem::size_of;

use pinocchio::cpi::{Seed, Signer};
use pinocchio::error::ProgramError;
use pinocchio::sysvars::rent::Rent;
use pinocchio::sysvars::Sysvar;
use pinocchio::{AccountView, Address, ProgramResult};
use pinocchio_associated_token_account::instructions::Create as CreateAssociatedTokenAccount;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::state::{Mint, TokenAccount};

use crate::state::{Escrow, EscrowSnapshot};
use crate::{EscrowError, ID};

pub(crate) const ESCROW_SEED: &[u8] = b"escrow";

const CREATE_IX_DATA_LEN: usize = size_of::<u64>() * 3;
const TOKEN_AMOUNT_OFFSET: usize = 64;
const TOKEN_AMOUNT_END: usize = TOKEN_AMOUNT_OFFSET + size_of::<u64>();

#[derive(Clone, Copy)]
pub(crate) struct CreateInstructionData {
    pub(crate) seed: u64,
    pub(crate) receive: u64,
    pub(crate) amount: u64,
}

pub(crate) fn parse_create_ix_data(data: &[u8]) -> Result<CreateInstructionData, ProgramError> {
    if data.len() != CREATE_IX_DATA_LEN {
        return Err(EscrowError::InvalidInstruction.into());
    }

    let seed = read_u64_le(&data[0..8])?;
    let receive = read_u64_le(&data[8..16])?;
    let amount = read_u64_le(&data[16..24])?;

    if receive == 0 || amount == 0 {
        return Err(EscrowError::InvalidAmount.into());
    }

    Ok(CreateInstructionData {
        seed,
        receive,
        amount,
    })
}

fn read_u64_le(bytes: &[u8]) -> Result<u64, ProgramError> {
    let buf: [u8; 8] = bytes
        .try_into()
        .map_err(|_| ProgramError::from(EscrowError::InvalidInstruction))?;
    Ok(u64::from_le_bytes(buf))
}

pub(crate) fn load_and_validate_escrow(
    escrow_account: &AccountView,
    maker: &AccountView,
    mint_a: &AccountView,
    mint_b: Option<&AccountView>,
) -> Result<EscrowSnapshot, ProgramError> {
    let escrow_data = escrow_account.try_borrow()?;
    let escrow = Escrow::load(&escrow_data)?;

    if escrow.maker.ne(maker.address()) || escrow.mint_a.ne(mint_a.address()) {
        return Err(EscrowError::InvalidEscrowState.into());
    }

    if let Some(mint_b_account) = mint_b {
        if escrow.mint_b.ne(mint_b_account.address()) {
            return Err(EscrowError::InvalidEscrowState.into());
        }
    }

    let seed_bytes = escrow.seed.to_le_bytes();
    let expected_escrow = Address::create_program_address(
        &[
            ESCROW_SEED,
            maker.address().as_ref(),
            &seed_bytes,
            &escrow.bump,
        ],
        &ID,
    )
    .map_err(|_| ProgramError::from(EscrowError::InvalidAddress))?;

    if expected_escrow.ne(escrow_account.address()) {
        return Err(EscrowError::InvalidAddress.into());
    }

    Ok(EscrowSnapshot {
        seed: escrow.seed,
        receive: escrow.receive,
        bump: escrow.bump,
    })
}

pub(crate) fn token_account_amount(token_account: &AccountView) -> Result<u64, ProgramError> {
    let token_data = token_account.try_borrow()?;
    if token_data.len() < TOKEN_AMOUNT_END {
        return Err(EscrowError::InvalidAccountData.into());
    }

    let amount_bytes: [u8; 8] = token_data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_END]
        .try_into()
        .map_err(|_| ProgramError::from(EscrowError::InvalidAccountData))?;

    Ok(u64::from_le_bytes(amount_bytes))
}

pub(crate) fn create_program_account<'a>(
    payer: &AccountView,
    account: &AccountView,
    seeds: &[Seed<'a>],
    space: usize,
) -> ProgramResult {
    let lamports = Rent::get()?.try_minimum_balance(space)?;
    let signer = [Signer::from(seeds)];

    CreateAccount {
        from: payer,
        to: account,
        lamports,
        space: space as u64,
        owner: &ID,
    }
    .invoke_signed(&signer)
}

pub(crate) fn close_program_account(account: &AccountView, destination: &AccountView) -> ProgramResult {
    {
        let mut data = account.try_borrow_mut()?;
        data[0] = 0xff;
    }

    destination.set_lamports(destination.lamports() + account.lamports());
    account.resize(1)?;
    account.close()
}

pub(crate) fn init_ata(
    ata: &AccountView,
    mint: &AccountView,
    payer: &AccountView,
    owner: &AccountView,
    system_program: &AccountView,
    token_program: &AccountView,
) -> ProgramResult {
    CreateAssociatedTokenAccount {
        funding_account: payer,
        account: ata,
        wallet: owner,
        mint,
        system_program,
        token_program,
    }
    .invoke()
}

pub(crate) fn init_ata_if_needed(
    ata: &AccountView,
    mint: &AccountView,
    payer: &AccountView,
    owner: &AccountView,
    system_program: &AccountView,
    token_program: &AccountView,
) -> ProgramResult {
    assert_ata_address(ata, owner, mint, token_program)?;

    if ata.data_len().eq(&0) {
        init_ata(ata, mint, payer, owner, system_program, token_program)
    } else {
        assert_token_account(ata, token_program)
    }
}

pub(crate) fn assert_signer(account: &AccountView) -> ProgramResult {
    if !account.is_signer() {
        return Err(EscrowError::MissingRequiredSignature.into());
    }
    Ok(())
}

fn assert_program(account: &AccountView, expected: &Address) -> ProgramResult {
    if account.address().ne(expected) {
        return Err(EscrowError::InvalidProgram.into());
    }
    Ok(())
}

pub(crate) fn assert_system_program(system_program: &AccountView) -> ProgramResult {
    assert_program(system_program, &pinocchio_system::ID)
}

pub(crate) fn assert_token_program(token_program: &AccountView) -> ProgramResult {
    assert_program(token_program, &pinocchio_token::ID)
}

fn assert_owned_by(account: &AccountView, owner: &Address) -> ProgramResult {
    if !account.owned_by(owner) {
        return Err(EscrowError::InvalidOwner.into());
    }
    Ok(())
}

fn assert_data_len(account: &AccountView, expected_len: usize) -> ProgramResult {
    if account.data_len().ne(&expected_len) {
        return Err(EscrowError::InvalidAccountData.into());
    }
    Ok(())
}

pub(crate) fn assert_program_account(
    account: &AccountView,
    expected_len: usize,
    owner: &Address,
) -> ProgramResult {
    assert_owned_by(account, owner)?;
    assert_data_len(account, expected_len)
}

pub(crate) fn assert_mint_account(mint: &AccountView, token_program: &AccountView) -> ProgramResult {
    assert_owned_by(mint, token_program.address())?;
    assert_data_len(mint, Mint::LEN)
}

fn assert_token_account(token_account: &AccountView, token_program: &AccountView) -> ProgramResult {
    assert_owned_by(token_account, token_program.address())?;
    assert_data_len(token_account, TokenAccount::LEN)
}

pub(crate) fn assert_ata_address(
    ata: &AccountView,
    authority: &AccountView,
    mint: &AccountView,
    token_program: &AccountView,
) -> ProgramResult {
    let (expected, _) = Address::find_program_address(
        &[
            authority.address().as_ref(),
            token_program.address().as_ref(),
            mint.address().as_ref(),
        ],
        &pinocchio_associated_token_account::ID,
    );

    if expected.ne(ata.address()) {
        return Err(EscrowError::InvalidAddress.into());
    }

    Ok(())
}

pub(crate) fn assert_ata_account(
    ata: &AccountView,
    authority: &AccountView,
    mint: &AccountView,
    token_program: &AccountView,
) -> ProgramResult {
    assert_ata_address(ata, authority, mint, token_program)?;
    assert_token_account(ata, token_program)
}
