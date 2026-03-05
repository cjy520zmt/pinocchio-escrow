# Pinocchio Escrow Client

这个目录是为 `src/lib.rs` 对应的 Solana 托管程序实现的完整 Web 客户端（Next.js + TypeScript + Solana Wallet Adapter）。

## 功能

- 钱包连接（Phantom / Solflare）
- 显示当前连接钱包地址
- `Make`（discriminator = `0`）创建托管
- `Take`（discriminator = `1`）接受托管
- `Refund`（discriminator = `2`）退款
- 列出并查看当前 Program 下 escrow 状态（支持按 maker 过滤）
- 将链上 `EscrowError` 自定义错误码映射为可读提示

## 项目结构

- `app/`：Next.js 页面与全局样式
- `components/WalletContextProvider.tsx`：钱包与 RPC Provider
- `components/EscrowClient.tsx`：主交互界面
- `lib/constants.ts`：RPC / Program 常量
- `lib/escrow.ts`：PDA 推导、指令编码、状态解码、错误映射

## 快速开始

1. 进入客户端目录

```bash
cd client
```

2. 安装依赖

```bash
npm install
```

3. 配置环境变量

```bash
cp .env.example .env.local
```

默认配置：

- `NEXT_PUBLIC_SOLANA_RPC_URL=https://api.devnet.solana.com`
- `NEXT_PUBLIC_ESCROW_PROGRAM_ID=9Ac37wYboPRXYEmg44Npsw9D3jYbFn4dxqZMKuyAQQvH`

4. 启动开发服务器

```bash
npm run dev
```

5. 打开浏览器

- `http://localhost:3000`

## 指令编码说明

客户端按程序要求构造了以下 instruction data：

- `Make`: `[0, seed(u64_le), receive(u64_le), amount(u64_le)]`
- `Take`: `[1]`
- `Refund`: `[2]`

## Escrow 状态布局

客户端按 `src/state.rs` 解析 escrow 账户（长度 113 字节）：

- `seed: u64`（offset 0）
- `maker: Pubkey`（offset 8）
- `mint_a: Pubkey`（offset 40）
- `mint_b: Pubkey`（offset 72）
- `receive: u64`（offset 104）
- `bump: u8`（offset 112）

## 错误处理

`EscrowError` 映射（自定义错误码）：

- `0`: MissingRequiredSignature
- `1`: InvalidProgram
- `2`: InvalidOwner
- `3`: InvalidAccountData
- `4`: InvalidInstruction
- `5`: InvalidAmount
- `6`: InvalidAddress
- `7`: InvalidEscrowState

客户端会在交易报错时解析 `custom program error` 并显示对应含义。

## 使用前提

- 钱包需要有 SOL 支付手续费
- 钱包下要有参与交易所需的 SPL Token（按最小单位输入）
- `Make` 时 `Mint A` 对应 ATA 必须存在且余额充足

