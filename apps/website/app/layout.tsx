import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Industrialis — GT New Horizons Launcher",
  description:
    "Desktop launcher for GT New Horizons. Install instances, manage Java, sign in with Microsoft, and play.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}