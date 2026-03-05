import { PublicKey } from "@solana/web3.js";

// 默认连接 Devnet，便于学习和测试。
export const DEFAULT_RPC_ENDPOINT = "https://api.devnet.solana.com";
export const RPC_ENDPOINT = process.env.NEXT_PUBLIC_SOLANA_RPC_URL ?? DEFAULT_RPC_ENDPOINT;

// 默认指向本仓库对应的 escrow 程序，可通过环境变量覆盖。
export const DEFAULT_ESCROW_PROGRAM_ID = "9Ac37wYboPRXYEmg44Npsw9D3jYbFn4dxqZMKuyAQQvH";
export const ESCROW_PROGRAM_ID = new PublicKey(
  process.env.NEXT_PUBLIC_ESCROW_PROGRAM_ID ?? DEFAULT_ESCROW_PROGRAM_ID,
);

// 需要与链上 `src/instructions/helpers.rs` 与 `src/state.rs` 保持一致。
export const ESCROW_SEED = "escrow";
export const ESCROW_ACCOUNT_SIZE = 113;
