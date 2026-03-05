"use client";

import { type FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { PublicKey, type Transaction } from "@solana/web3.js";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { WalletMultiButton } from "@solana/wallet-adapter-react-ui";

import { ESCROW_PROGRAM_ID, RPC_ENDPOINT } from "@/lib/constants";
import {
  buildMakeTransaction,
  buildRefundTransaction,
  buildTakeTransaction,
  type EscrowState,
  fetchEscrows,
  formatEscrowError,
  parseU64Input,
} from "@/lib/escrow";

type NoticeType = "info" | "success" | "error";

interface Notice {
  type: NoticeType;
  message: string;
}

interface ActionResult {
  signature: string;
  detail?: string;
}

function parsePublicKey(value: string, label: string): PublicKey {
  try {
    return new PublicKey(value.trim());
  } catch {
    throw new Error(`${label} 不是有效的 Solana 地址`);
  }
}

function shortAddress(value: string, size = 6): string {
  if (value.length <= size * 2 + 3) {
    return value;
  }
  return `${value.slice(0, size)}...${value.slice(-size)}`;
}

function explorerTxUrl(signature: string): string {
  const endpoint = RPC_ENDPOINT.toLowerCase();
  if (endpoint.includes("devnet")) {
    return `https://explorer.solana.com/tx/${signature}?cluster=devnet`;
  }
  if (endpoint.includes("testnet")) {
    return `https://explorer.solana.com/tx/${signature}?cluster=testnet`;
  }
  if (endpoint.includes("mainnet")) {
    return `https://explorer.solana.com/tx/${signature}`;
  }
  return `https://explorer.solana.com/tx/${signature}?cluster=custom&customUrl=${encodeURIComponent(
    RPC_ENDPOINT,
  )}`;
}

function seedSortDesc(a: EscrowState, b: EscrowState): number {
  if (a.seed === b.seed) {
    return 0;
  }
  return a.seed > b.seed ? -1 : 1;
}

export function EscrowClient() {
  const { connection } = useConnection();
  const { publicKey, connected, sendTransaction } = useWallet();

  const [notice, setNotice] = useState<Notice | null>(null);
  const [lastSignature, setLastSignature] = useState<string | null>(null);
  const [activeActionKey, setActiveActionKey] = useState<string | null>(null);

  const [makeSeed, setMakeSeed] = useState("1");
  const [makeMintA, setMakeMintA] = useState("");
  const [makeMintB, setMakeMintB] = useState("");
  const [makeAmount, setMakeAmount] = useState("1");
  const [makeReceive, setMakeReceive] = useState("1");

  const [makerFilter, setMakerFilter] = useState("");
  const [loadingEscrows, setLoadingEscrows] = useState(false);
  const [escrows, setEscrows] = useState<EscrowState[]>([]);

  const walletAddress = useMemo(() => publicKey?.toBase58() ?? "未连接", [publicKey]);

  useEffect(() => {
    if (publicKey && makerFilter === "") {
      setMakerFilter(publicKey.toBase58());
    }
  }, [publicKey, makerFilter]);

  const submitTransaction = useCallback(
    async (transaction: Transaction): Promise<string> => {
      if (!publicKey) {
        throw new Error("请先连接钱包");
      }

      const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash("confirmed");
      transaction.feePayer = publicKey;
      transaction.recentBlockhash = blockhash;

      const signature = await sendTransaction(transaction, connection, {
        preflightCommitment: "confirmed",
      });

      await connection.confirmTransaction(
        {
          signature,
          blockhash,
          lastValidBlockHeight,
        },
        "confirmed",
      );

      return signature;
    },
    [connection, publicKey, sendTransaction],
  );

  const loadEscrows = useCallback(
    async (filterInput: string) => {
      setLoadingEscrows(true);

      try {
        const trimmed = filterInput.trim();
        const maker = trimmed.length > 0 ? parsePublicKey(trimmed, "Maker 过滤地址") : undefined;
        const rows = await fetchEscrows(connection, maker);
        rows.sort(seedSortDesc);
        setEscrows(rows);
      } finally {
        setLoadingEscrows(false);
      }
    },
    [connection],
  );

  useEffect(() => {
    void loadEscrows("").catch((error) => {
      setNotice({ type: "error", message: formatEscrowError(error) });
    });
  }, [loadEscrows]);

  const runAction = useCallback(
    async (actionKey: string, taskName: string, run: () => Promise<ActionResult>) => {
      setActiveActionKey(actionKey);
      setLastSignature(null);
      setNotice({ type: "info", message: `${taskName} 交易发送中...` });

      try {
        const result = await run();
        setLastSignature(result.signature);
        setNotice({
          type: "success",
          message: result.detail ?? `${taskName} 成功，签名: ${result.signature}`,
        });
        await loadEscrows(makerFilter).catch(() => undefined);
      } catch (error) {
        setNotice({ type: "error", message: formatEscrowError(error) });
      } finally {
        setActiveActionKey(null);
      }
    },
    [loadEscrows, makerFilter],
  );

  const onMake = useCallback(
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (!publicKey) {
        setNotice({ type: "error", message: "请先连接钱包" });
        return;
      }

      await runAction("make", "Make", async () => {
        const seed = parseU64Input(makeSeed, "Seed");
        const mintA = parsePublicKey(makeMintA, "Mint A");
        const mintB = parsePublicKey(makeMintB, "Mint B");
        const amount = parseU64Input(makeAmount, "托管数量 amount", false);
        const receive = parseU64Input(makeReceive, "交换数量 receive", false);

        const { transaction, escrow, vault } = buildMakeTransaction({
          maker: publicKey,
          seed,
          mintA,
          mintB,
          amount,
          receive,
        });

        const signature = await submitTransaction(transaction);

        return {
          signature,
          detail: `Make 成功。Escrow: ${escrow.toBase58()}，Vault: ${vault.toBase58()}，签名: ${signature}`,
        };
      });
    },
    [
      makeAmount,
      makeMintA,
      makeMintB,
      makeReceive,
      makeSeed,
      publicKey,
      runAction,
      submitTransaction,
    ],
  );

  const onTake = useCallback(
    async (escrow: EscrowState) => {
      if (!publicKey) {
        setNotice({ type: "error", message: "请先连接钱包" });
        return;
      }

      const actionKey = `take:${escrow.address.toBase58()}`;
      await runAction(actionKey, "Take", async () => {
        const transaction = buildTakeTransaction({
          taker: publicKey,
          maker: escrow.maker,
          seed: escrow.seed,
          mintA: escrow.mintA,
          mintB: escrow.mintB,
        });

        const signature = await submitTransaction(transaction);
        return {
          signature,
          detail: `Take 成功。Escrow: ${escrow.address.toBase58()}，签名: ${signature}`,
        };
      });
    },
    [publicKey, runAction, submitTransaction],
  );

  const onRefund = useCallback(
    async (escrow: EscrowState) => {
      if (!publicKey) {
        setNotice({ type: "error", message: "请先连接钱包" });
        return;
      }

      if (!publicKey.equals(escrow.maker)) {
        setNotice({ type: "error", message: "Refund 只能由创建 escrow 的 maker 执行" });
        return;
      }

      const actionKey = `refund:${escrow.address.toBase58()}`;
      await runAction(actionKey, "Refund", async () => {
        const transaction = buildRefundTransaction({
          maker: publicKey,
          seed: escrow.seed,
          mintA: escrow.mintA,
        });

        const signature = await submitTransaction(transaction);
        return {
          signature,
          detail: `Refund 成功。Escrow: ${escrow.address.toBase58()}，签名: ${signature}`,
        };
      });
    },
    [publicKey, runAction, submitTransaction],
  );

  const onRefreshClick = useCallback(() => {
    void (async () => {
      try {
        await loadEscrows(makerFilter);
      } catch (error) {
        setNotice({ type: "error", message: formatEscrowError(error) });
      }
    })();
  }, [loadEscrows, makerFilter]);

  const busy = activeActionKey !== null;

  return (
    <div className="escrow-page">
      <section className="hero">
        <p className="eyebrow">Pinocchio Escrow Client</p>
        <h1>Solana 托管交易面板</h1>
        <p>
          Program ID: <code>{ESCROW_PROGRAM_ID.toBase58()}</code>
        </p>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2>钱包连接</h2>
          <WalletMultiButton />
        </div>
        <p>
          当前钱包: <code>{walletAddress}</code>
        </p>
        <p>
          RPC Endpoint: <code>{RPC_ENDPOINT}</code>
        </p>
      </section>

      {notice && (
        <section className={`notice notice-${notice.type}`}>
          <strong>{notice.type.toUpperCase()}</strong>
          <span>{notice.message}</span>
          {lastSignature && (
            <a href={explorerTxUrl(lastSignature)} target="_blank" rel="noreferrer">
              在 Explorer 查看 {shortAddress(lastSignature, 8)}
            </a>
          )}
        </section>
      )}

      <section className="panel">
        <h2>1. Make（创建托管）</h2>
        <p className="hint">数量请填写 token 最小单位（raw amount），不是 UI 小数金额。</p>
        <form className="grid-form" onSubmit={(event) => void onMake(event)}>
          <label>
            Seed (u64)
            <input value={makeSeed} onChange={(event) => setMakeSeed(event.target.value)} />
          </label>
          <label>
            Mint A（托管资产）
            <input
              placeholder="例如 So11111111111111111111111111111111111111112"
              value={makeMintA}
              onChange={(event) => setMakeMintA(event.target.value)}
            />
          </label>
          <label>
            Mint B（对价资产）
            <input
              placeholder="目标 token mint 地址"
              value={makeMintB}
              onChange={(event) => setMakeMintB(event.target.value)}
            />
          </label>
          <label>
            Amount (u64)
            <input value={makeAmount} onChange={(event) => setMakeAmount(event.target.value)} />
          </label>
          <label>
            Receive (u64)
            <input value={makeReceive} onChange={(event) => setMakeReceive(event.target.value)} />
          </label>
          <button type="submit" disabled={!connected || busy}>
            {activeActionKey === "make" ? "提交中..." : "发送 Make"}
          </button>
        </form>
      </section>

      <section className="panel">
        <h2>2. 查看现有托管状态</h2>
        <div className="toolbar">
          <input
            placeholder="可选：按 Maker 地址过滤"
            value={makerFilter}
            onChange={(event) => setMakerFilter(event.target.value)}
          />
          <button type="button" onClick={onRefreshClick} disabled={loadingEscrows || busy}>
            {loadingEscrows ? "刷新中..." : "刷新列表"}
          </button>
        </div>

        {loadingEscrows && <p>正在加载 escrow 列表...</p>}
        {!loadingEscrows && escrows.length === 0 && <p>当前没有匹配的 escrow。</p>}

        <div className="escrow-list">
          {escrows.map((item) => {
            const address = item.address.toBase58();
            const takeKey = `take:${address}`;
            const refundKey = `refund:${address}`;
            const canRefund = publicKey ? publicKey.equals(item.maker) : false;

            return (
              <article className="escrow-card" key={address}>
                <h3>{shortAddress(address, 10)}</h3>
                <p>
                  Escrow: <code>{address}</code>
                </p>
                <p>
                  Maker: <code>{item.maker.toBase58()}</code>
                </p>
                <p>
                  Seed: <code>{item.seed.toString()}</code>
                </p>
                <p>
                  Mint A: <code>{item.mintA.toBase58()}</code>
                </p>
                <p>
                  Mint B: <code>{item.mintB.toBase58()}</code>
                </p>
                <p>
                  Receive(B): <code>{item.receive.toString()}</code>
                </p>
                <p>
                  Vault: <code>{item.vault.toBase58()}</code>
                </p>
                <p>
                  Vault Amount(A): <code>{item.vaultAmount.toString()}</code>
                </p>

                <div className="card-actions">
                  <button
                    type="button"
                    className="secondary"
                    disabled={!connected || busy}
                    onClick={() => void onTake(item)}
                  >
                    {activeActionKey === takeKey ? "Take 处理中..." : "Take"}
                  </button>
                  <button
                    type="button"
                    className="danger"
                    disabled={!connected || busy || !canRefund}
                    onClick={() => void onRefund(item)}
                    title={canRefund ? "" : "仅 maker 可执行 refund"}
                  >
                    {activeActionKey === refundKey ? "Refund 处理中..." : "Refund"}
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      </section>
    </div>
  );
}
