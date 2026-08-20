import type { Metadata } from "next";
import { JetBrains_Mono, Plus_Jakarta_Sans } from "next/font/google";
import "./globals.css";

const sans = Plus_Jakarta_Sans({
  subsets: ["latin"],
  variable: "--font-sans-family",
});

const mono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-mono-family",
});

export const metadata: Metadata = {
  title: "Industrialis - GT New Horizons Launcher",
  description: "A Windows launcher for GT New Horizons. Manage instances, Java, memory, accounts, and launch logs in one focused library.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className={`${sans.variable} ${mono.variable}`}>
      <body className="font-sans">{children}</body>
    </html>
  );
}
