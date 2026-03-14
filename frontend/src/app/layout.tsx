import type { Metadata } from "next"
import "./globals.css"

export const metadata: Metadata = {
  title: "MiroFish - Swarm Intelligence Engine",
  description: "Upload any report, simulate the future instantly",
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
