"use client";

import { useState, useEffect } from "react";
import Link from "next/link";
import { getStats, type StatsResponse } from "@/lib/api";

const STROOPS_PER_XLM = 10_000_000;

function formatXlm(stroops: number): string {
  if (!stroops) return "0 XLM";
  const xlm = stroops / STROOPS_PER_XLM;
  return `${xlm.toLocaleString(undefined, { maximumFractionDigits: 2 })} XLM`;
}

function shortAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

export default function StatsPage() {
  const [stats, setStats] = useState<StatsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const data = await getStats();
        if (!cancelled) setStats(data);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Failed to load stats");
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    load();
    // Refresh every 30 s to match the server-side cache TTL.
    const id = setInterval(load, 30_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  return (
    <main className="min-h-screen bg-gray-950 text-gray-100 p-6 font-mono">
      <div className="max-w-2xl mx-auto">
        <div className="flex items-center justify-between mb-6">
          <h1 className="text-2xl font-bold text-yellow-400">📊 Onchain Stats</h1>
          <Link href="/" className="text-sm text-gray-400 hover:text-gray-200 underline">
            ← Back
          </Link>
        </div>

        {loading && (
          <p className="text-gray-400 animate-pulse">Loading stats…</p>
        )}

        {error && (
          <p className="text-red-400 bg-red-900/30 rounded px-4 py-2">{error}</p>
        )}

        {stats && (
          <>
            {/* Global stats */}
            <section className="mb-8">
              <h2 className="text-lg font-semibold text-gray-300 mb-3 uppercase tracking-wide">
                Global
              </h2>
              <div className="grid grid-cols-3 gap-4">
                <StatCard label="Hands Played" value={stats.global.hands_played.toLocaleString()} />
                <StatCard label="Biggest Pot" value={formatXlm(stats.global.biggest_pot)} />
                <StatCard label="Players Joined" value={stats.global.total_players_joined.toLocaleString()} />
              </div>
            </section>

            {/* Leaderboard */}
            <section>
              <h2 className="text-lg font-semibold text-gray-300 mb-3 uppercase tracking-wide">
                Leaderboard
              </h2>
              {stats.leaderboard.length === 0 ? (
                <p className="text-gray-500 text-sm">No hands played yet.</p>
              ) : (
                <table className="w-full text-sm border-collapse">
                  <thead>
                    <tr className="text-gray-400 border-b border-gray-700">
                      <th className="text-left py-2 pr-4">#</th>
                      <th className="text-left py-2 pr-4">Player</th>
                      <th className="text-right py-2 pr-4">Hands Won</th>
                      <th className="text-right py-2 pr-4">Hands Played</th>
                      <th className="text-right py-2">Biggest Pot Won</th>
                    </tr>
                  </thead>
                  <tbody>
                    {stats.leaderboard.map((p, i) => (
                      <tr
                        key={p.address}
                        className={`border-b border-gray-800 ${i === 0 ? "text-yellow-300" : "text-gray-200"}`}
                      >
                        <td className="py-2 pr-4 text-gray-500">{i + 1}</td>
                        <td className="py-2 pr-4 font-mono" title={p.address}>
                          {shortAddress(p.address)}
                        </td>
                        <td className="py-2 pr-4 text-right">{p.hands_won}</td>
                        <td className="py-2 pr-4 text-right text-gray-400">{p.hands_played}</td>
                        <td className="py-2 text-right text-gray-400">
                          {formatXlm(p.biggest_pot_won)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </section>

            <p className="mt-6 text-xs text-gray-600">
              Cached at {new Date(stats.cached_at * 1000).toLocaleTimeString()} · refreshes every 30s
            </p>
          </>
        )}
      </div>
    </main>
  );
}

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-gray-900 rounded-lg p-4 border border-gray-800">
      <p className="text-xs text-gray-500 mb-1">{label}</p>
      <p className="text-xl font-bold text-white">{value}</p>
    </div>
  );
}
