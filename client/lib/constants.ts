import { PublicKey } from "@solana/web3.js";

export const DEFAULT_RPC_ENDPOINT = "https://api.devnet.solana.com";
export const RPC_ENDPOINT = process.env.NEXT_PUBLIC_SOLANA_RPC_URL ?? DEFAULT_RPC_ENDPOINT;

export const DEFAULT_ESCROW_PROGRAM_ID = "9Ac37wYboPRXYEmg44Npsw9D3jYbFn4dxqZMKuyAQQvH";
export const ESCROW_PROGRAM_ID = new PublicKey(
  process.env.NEXT_PUBLIC_ESCROW_PROGRAM_ID ?? DEFAULT_ESCROW_PROGRAM_ID,
);

export const ESCROW_SEED = "escrow";
export const ESCROW_ACCOUNT_SIZE = 113;
