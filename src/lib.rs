use core::mem::size_of;

use pinocchio::cpi::{Seed, Signer};
use pinocchio::error::ProgramError;
use pinocchio::sysvars::rent::Rent;
use pinocchio::sysvars::Sysvar;
use pinocchio::{entrypoint, AccountView, Address, ProgramResult};
use pinocchio_associated_token_account::instructions::Create as CreateAssociatedTokenAccount;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::{CloseAccount, Transfer};
use pinocchio_token::state::{Mint, TokenAccount};

entrypoint!(process_instruction);

// 22222222222222222222222222222222222222222222
pub const ID: Address = Address::new_from_array([
    0x0f, 0x1e, 0x6b, 0x14, 0x21, 0xc0, 0x4a, 0x07, 0x04, 0x31, 0x26, 0x5c, 0x19, 0xc5, 0xbb, 0xee,
    0x19, 0x92, 0xba, 0xe8, 0xaf, 0xd1, 0xcd, 0x07, 0x8e, 0xf8, 0xaf, 0x70, 0x47, 0xdc, 0x11, 0xf7,
]);

const ESCROW_SEED: &[u8] = b"escrow";
const CREATE_IX_DATA_LEN: usize = size_of::<u64>() * 3;
const TOKEN_AMOUNT_OFFSET: usize = 64;
const TOKEN_AMOUNT_END: usize = TOKEN_AMOUNT_OFFSET + size_of::<u64>();

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
struct CreateInstructionData {
    seed: u64,
    receive: u64,
    amount: u64,
}

#[derive(Clone, Copy)]
struct EscrowSnapshot {
    seed: u64,
    receive: u64,
    bump: [u8; 1],
}

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
        0 => create(accounts, data),
        1 => {
            if !data.is_empty() {
                return Err(EscrowError::InvalidInstruction.into());
            }
            accept(accounts)
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

fn create(accounts: &[AccountView], data: &[u8]) -> ProgramResult {
    let ix = parse_create_ix_data(data)?;

    let [maker, escrow, mint_a, mint_b, maker_ata_a, vault, system_program, token_program, ..] =
        accounts
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

fn accept(accounts: &[AccountView]) -> ProgramResult {
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

    let escrow_state = load_and_validate_escrow(escrow, maker, mint_a, Some(mint_b))?;

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

    let seed_bytes = escrow_state.seed.to_le_bytes();
    let bump_bytes = escrow_state.bump;
    let escrow_seeds = [
        Seed::from(ESCROW_SEED),
        Seed::from(maker.address().as_ref()),
        Seed::from(&seed_bytes),
        Seed::from(&bump_bytes),
    ];
    let signer = Signer::from(&escrow_seeds);

    // Fail-fast on payment leg before releasing vault funds.
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

    close_program_account(escrow, maker)
}

fn refund(accounts: &[AccountView]) -> ProgramResult {
    let [maker, escrow, mint_a, vault, maker_ata_a, system_program, token_program, ..] = accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    assert_signer(maker)?;
    assert_system_program(system_program)?;
    assert_token_program(token_program)?;

    assert_program_account(escrow, Escrow::LEN, &ID)?;
    assert_mint_account(mint_a, token_program)?;
    assert_ata_account(vault, escrow, mint_a, token_program)?;

    let escrow_state = load_and_validate_escrow(escrow, maker, mint_a, None)?;

    init_ata_if_needed(
        maker_ata_a,
        mint_a,
        maker,
        maker,
        system_program,
        token_program,
    )?;

    let vault_amount = token_account_amount(vault)?;

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

    close_program_account(escrow, maker)
}

fn parse_create_ix_data(data: &[u8]) -> Result<CreateInstructionData, ProgramError> {
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

fn load_and_validate_escrow(
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

fn token_account_amount(token_account: &AccountView) -> Result<u64, ProgramError> {
    let token_data = token_account.try_borrow()?;
    if token_data.len() < TOKEN_AMOUNT_END {
        return Err(EscrowError::InvalidAccountData.into());
    }

    let amount_bytes: [u8; 8] = token_data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_END]
        .try_into()
        .map_err(|_| ProgramError::from(EscrowError::InvalidAccountData))?;

    Ok(u64::from_le_bytes(amount_bytes))
}

fn create_program_account<'a>(
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

fn close_program_account(account: &AccountView, destination: &AccountView) -> ProgramResult {
    {
        let mut data = account.try_borrow_mut()?;
        data[0] = 0xff;
    }

    destination.set_lamports(destination.lamports() + account.lamports());
    account.resize(1)?;
    account.close()
}

fn init_ata(
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

fn init_ata_if_needed(
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

fn assert_signer(account: &AccountView) -> ProgramResult {
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

fn assert_system_program(system_program: &AccountView) -> ProgramResult {
    assert_program(system_program, &pinocchio_system::ID)
}

fn assert_token_program(token_program: &AccountView) -> ProgramResult {
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

fn assert_program_account(
    account: &AccountView,
    expected_len: usize,
    owner: &Address,
) -> ProgramResult {
    assert_owned_by(account, owner)?;
    assert_data_len(account, expected_len)
}

fn assert_mint_account(mint: &AccountView, token_program: &AccountView) -> ProgramResult {
    assert_owned_by(mint, token_program.address())?;
    assert_data_len(mint, Mint::LEN)
}

fn assert_token_account(token_account: &AccountView, token_program: &AccountView) -> ProgramResult {
    assert_owned_by(token_account, token_program.address())?;
    assert_data_len(token_account, TokenAccount::LEN)
}

fn assert_ata_address(
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

fn assert_ata_account(
    ata: &AccountView,
    authority: &AccountView,
    mint: &AccountView,
    token_program: &AccountView,
) -> ProgramResult {
    assert_ata_address(ata, authority, mint, token_program)?;
    assert_token_account(ata, token_program)
}
