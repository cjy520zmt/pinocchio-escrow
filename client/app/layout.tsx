import type { Metadata } from "next";
import type { ReactNode } from "react";

import { WalletContextProvider } from "@/components/WalletContextProvider";
import "./globals.css";

export const metadata: Metadata = {
  title: "Pinocchio Escrow Client",
  description: "Web client for Make / Take / Refund escrow instructions",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: ReactNode;
}>) {
  return (
    <html lang="zh-CN">
      <body>
        <WalletContextProvider>{children}</WalletContextProvider>
      </body>
    </html>
  );
}
