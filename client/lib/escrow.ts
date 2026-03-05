import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";

import { ESCROW_ACCOUNT_SIZE, ESCROW_PROGRAM_ID, ESCROW_SEED } from "@/lib/constants";

// Token 数量、seed 等字段都来自链上 u64。
const U64_MAX = (1n << 64n) - 1n;
const TOKEN_ACCOUNT_AMOUNT_OFFSET = 64;

// 与 `src/errors.rs` 的 EscrowError 一一对应。
const ESCROW_ERROR_MESSAGES: Record<number, string> = {
  0: "缺少必要签名（MissingRequiredSignature）",
  1: "程序地址错误（InvalidProgram）",
  2: "账户 owner 不匹配（InvalidOwner）",
  3: "账户数据不合法（InvalidAccountData）",
  4: "指令数据不合法（InvalidInstruction）",
  5: "金额无效（InvalidAmount）",
  6: "地址不匹配（InvalidAddress）",
  7: "托管状态无效（InvalidEscrowState）",
};

export interface EscrowDecoded {
  address: PublicKey;
  seed: bigint;
  maker: PublicKey;
  mintA: PublicKey;
  mintB: PublicKey;
  receive: bigint;
  bump: number;
  vault: PublicKey;
}

export interface EscrowState extends EscrowDecoded {
  // 从 vault token account 读取的实时余额（mint_a）。
  vaultAmount: bigint;
}

interface MakeTransactionParams {
  maker: PublicKey;
  seed: bigint;
  mintA: PublicKey;
  mintB: PublicKey;
  amount: bigint;
  receive: bigint;
}

interface TakeTransactionParams {
  taker: PublicKey;
  maker: PublicKey;
  seed: bigint;
  mintA: PublicKey;
  mintB: PublicKey;
}

interface RefundTransactionParams {
  maker: PublicKey;
  seed: bigint;
  mintA: PublicKey;
}

function ensureU64(value: bigint, label: string, allowZero = true): bigint {
  if (value < 0n || value > U64_MAX) {
    throw new Error(`${label} 必须在 u64 范围内`);
  }
  if (!allowZero && value === 0n) {
    throw new Error(`${label} 不能为 0`);
  }
  return value;
}

// 表单输入统一转成 bigint，并做 u64/非零校验。
export function parseU64Input(input: string, label: string, allowZero = true): bigint {
  const trimmed = input.trim();
  if (trimmed.length === 0) {
    throw new Error(`${label} 不能为空`);
  }
  if (!/^\d+$/.test(trimmed)) {
    throw new Error(`${label} 必须是无符号整数（token 最小单位）`);
  }
  return ensureU64(BigInt(trimmed), label, allowZero);
}

function u64ToLeBuffer(value: bigint, label: string, allowZero = true): Buffer {
  const checked = ensureU64(value, label, allowZero);
  const buffer = Buffer.alloc(8);
  buffer.writeBigUInt64LE(checked, 0);
  return buffer;
}

// escrow PDA = ["escrow", maker, seed_le_bytes]。
export function deriveEscrowPda(maker: PublicKey, seed: bigint): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from(ESCROW_SEED), maker.toBuffer(), u64ToLeBuffer(seed, "seed")],
    ESCROW_PROGRAM_ID,
  );
}

// vault 是 escrow PDA 作为 owner 的 mint_a ATA。
export function deriveVaultAta(escrow: PublicKey, mintA: PublicKey): PublicKey {
  return getAssociatedTokenAddressSync(
    mintA,
    escrow,
    true,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
}

// 按链上 `Escrow` 结构体固定偏移解码（见 src/state.rs）。
export function decodeEscrowAccount(address: PublicKey, data: Buffer): EscrowDecoded {
  if (data.length !== ESCROW_ACCOUNT_SIZE) {
    throw new Error(`Escrow 账户大小错误，期望 ${ESCROW_ACCOUNT_SIZE}，实际 ${data.length}`);
  }

  const seed = data.readBigUInt64LE(0);
  const maker = new PublicKey(data.subarray(8, 40));
  const mintA = new PublicKey(data.subarray(40, 72));
  const mintB = new PublicKey(data.subarray(72, 104));
  const receive = data.readBigUInt64LE(104);
  const bump = data.readUInt8(112);
  const vault = deriveVaultAta(address, mintA);

  return {
    address,
    seed,
    maker,
    mintA,
    mintB,
    receive,
    bump,
    vault,
  };
}

function decodeTokenAmount(accountData: Buffer | null): bigint {
  if (!accountData || accountData.length < TOKEN_ACCOUNT_AMOUNT_OFFSET + 8) {
    return 0n;
  }
  return accountData.readBigUInt64LE(TOKEN_ACCOUNT_AMOUNT_OFFSET);
}

// 拉取程序下所有 escrow，并补齐每个 escrow 的 vault 实时余额。
export async function fetchEscrows(
  connection: Connection,
  makerFilter?: PublicKey,
): Promise<EscrowState[]> {
  const accounts = await connection.getProgramAccounts(ESCROW_PROGRAM_ID, {
    filters: [{ dataSize: ESCROW_ACCOUNT_SIZE }],
  });

  const decoded = accounts
    .map(({ pubkey, account }) => decodeEscrowAccount(pubkey, account.data))
    .filter((item) => (makerFilter ? item.maker.equals(makerFilter) : true));

  if (decoded.length === 0) {
    return [];
  }

  const vaultInfos = await connection.getMultipleAccountsInfo(decoded.map((item) => item.vault));

  return decoded.map((item, idx) => ({
    ...item,
    vaultAmount: decodeTokenAmount(vaultInfos[idx]?.data ?? null),
  }));
}

export function buildMakeTransaction(params: MakeTransactionParams): {
  transaction: Transaction;
  escrow: PublicKey;
  vault: PublicKey;
} {
  const seed = ensureU64(params.seed, "seed");
  const receive = ensureU64(params.receive, "receive", false);
  const amount = ensureU64(params.amount, "amount", false);

  const [escrow] = deriveEscrowPda(params.maker, seed);
  const makerAtaA = getAssociatedTokenAddressSync(
    params.mintA,
    params.maker,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  const vault = deriveVaultAta(escrow, params.mintA);

  const data = Buffer.alloc(25);
  data.writeUInt8(0, 0);
  data.writeBigUInt64LE(seed, 1);
  data.writeBigUInt64LE(receive, 9);
  data.writeBigUInt64LE(amount, 17);

  // keys 顺序必须严格匹配 `src/instructions/make.rs` 的解构顺序。
  const instruction = new TransactionInstruction({
    programId: ESCROW_PROGRAM_ID,
    keys: [
      { pubkey: params.maker, isSigner: true, isWritable: true },
      { pubkey: escrow, isSigner: false, isWritable: true },
      { pubkey: params.mintA, isSigner: false, isWritable: false },
      { pubkey: params.mintB, isSigner: false, isWritable: false },
      { pubkey: makerAtaA, isSigner: false, isWritable: true },
      { pubkey: vault, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data,
  });

  return {
    transaction: new Transaction().add(instruction),
    escrow,
    vault,
  };
}

export function buildTakeTransaction(params: TakeTransactionParams): Transaction {
  const seed = ensureU64(params.seed, "seed");
  const [escrow] = deriveEscrowPda(params.maker, seed);
  const vault = deriveVaultAta(escrow, params.mintA);

  const takerAtaA = getAssociatedTokenAddressSync(
    params.mintA,
    params.taker,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  const takerAtaB = getAssociatedTokenAddressSync(
    params.mintB,
    params.taker,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );
  const makerAtaB = getAssociatedTokenAddressSync(
    params.mintB,
    params.maker,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );

  // keys 顺序必须严格匹配 `src/instructions/take.rs` 的解构顺序。
  const instruction = new TransactionInstruction({
    programId: ESCROW_PROGRAM_ID,
    keys: [
      { pubkey: params.taker, isSigner: true, isWritable: true },
      { pubkey: params.maker, isSigner: false, isWritable: true },
      { pubkey: escrow, isSigner: false, isWritable: true },
      { pubkey: params.mintA, isSigner: false, isWritable: false },
      { pubkey: params.mintB, isSigner: false, isWritable: false },
      { pubkey: vault, isSigner: false, isWritable: true },
      { pubkey: takerAtaA, isSigner: false, isWritable: true },
      { pubkey: takerAtaB, isSigner: false, isWritable: true },
      { pubkey: makerAtaB, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([1]),
  });

  return new Transaction().add(instruction);
}

export function buildRefundTransaction(params: RefundTransactionParams): Transaction {
  const seed = ensureU64(params.seed, "seed");
  const [escrow] = deriveEscrowPda(params.maker, seed);
  const vault = deriveVaultAta(escrow, params.mintA);
  const makerAtaA = getAssociatedTokenAddressSync(
    params.mintA,
    params.maker,
    false,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
  );

  // keys 顺序必须严格匹配 `src/instructions/refund.rs` 的解构顺序。
  const instruction = new TransactionInstruction({
    programId: ESCROW_PROGRAM_ID,
    keys: [
      { pubkey: params.maker, isSigner: true, isWritable: true },
      { pubkey: escrow, isSigner: false, isWritable: true },
      { pubkey: params.mintA, isSigner: false, isWritable: false },
      { pubkey: vault, isSigner: false, isWritable: true },
      { pubkey: makerAtaA, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from([2]),
  });

  return new Transaction().add(instruction);
}

function collectErrorText(error: unknown): string {
  // 尽量提取嵌套错误中的 message/logs/cause，便于给用户可读提示。
  const queue: unknown[] = [error];
  const seen = new Set<unknown>();
  const parts: string[] = [];

  while (queue.length > 0) {
    const current = queue.shift();
    if (current === null || current === undefined) {
      continue;
    }

    if (typeof current === "string") {
      parts.push(current);
      continue;
    }

    if (typeof current === "number" || typeof current === "boolean") {
      parts.push(String(current));
      continue;
    }

    if (typeof current !== "object") {
      continue;
    }

    if (seen.has(current)) {
      continue;
    }
    seen.add(current);

    const record = current as {
      message?: unknown;
      logs?: unknown;
      cause?: unknown;
      error?: unknown;
      toString?: () => string;
    };

    if (typeof record.message === "string") {
      parts.push(record.message);
    }

    if (Array.isArray(record.logs)) {
      const logText = record.logs
        .filter((item): item is string => typeof item === "string")
        .join(" ");
      if (logText.length > 0) {
        parts.push(logText);
      }
    }

    if (record.cause !== undefined) {
      queue.push(record.cause);
    }
    if (record.error !== undefined) {
      queue.push(record.error);
    }

    if (typeof record.toString === "function") {
      const text = record.toString();
      if (text.length > 0 && text !== "[object Object]") {
        parts.push(text);
      }
    }
  }

  return parts.join(" | ");
}

function extractCustomCode(raw: string): number | null {
  // 兼容 web3.js 常见报错格式：hex、decimal、InstructionError(Custom(x))。
  const hexMatch = raw.match(/custom program error: 0x([0-9a-f]+)/i);
  if (hexMatch) {
    return Number.parseInt(hexMatch[1], 16);
  }

  const decimalMatch = raw.match(/custom program error: (\d+)/i);
  if (decimalMatch) {
    return Number.parseInt(decimalMatch[1], 10);
  }

  const instructionMatch = raw.match(/InstructionError\([^,]+,\s*Custom\((\d+)\)\)/i);
  if (instructionMatch) {
    return Number.parseInt(instructionMatch[1], 10);
  }

  return null;
}

export function formatEscrowError(error: unknown): string {
  const raw = collectErrorText(error);
  const code = extractCustomCode(raw);

  if (code !== null && ESCROW_ERROR_MESSAGES[code]) {
    return `EscrowError(${code}): ${ESCROW_ERROR_MESSAGES[code]}`;
  }

  return raw || "未知错误";
}
