use soroban_sdk::{Env, Vec};

use crate::types::*;

/// Compute the main pot and any side pots for the current hand.
///
/// Side pots are required whenever players go all-in for different total
/// amounts. Each pot is formed from a "layer" of contributions between two
/// consecutive distinct all-in (or final) commitment levels. A player is only
/// eligible to win a pot if they contributed the full amount of that layer —
/// i.e. their total `committed` for the hand is at least that layer's level —
/// which is exactly the Texas Hold'em rule that a player can only win the chips
/// they have matched.
///
/// The pots are returned in order, main pot first, followed by side pots from
/// the lowest all-in level upward. Folded players still contribute their chips
/// (dead money stays in the pots they helped build) but are never eligible to
/// win. The sum of all returned pot amounts always equals `table.pot`.
///
/// Reference: <https://en.wikipedia.org/wiki/Side_pot>
pub fn calculate_side_pots(env: &Env, table: &TableState) -> Result<Vec<SidePot>, PokerTableError> {
    let mut pots: Vec<SidePot> = Vec::new(env);

    // Distinct, ascending contribution levels at which a pot boundary occurs.
    // We only need a boundary where a *contender* (non-folded player) is capped
    // (all-in). Players who bet more than the highest such cap form the top
    // layer, which is handled by the trailing pass below.
    let levels = distinct_all_in_levels(env, table)?;

    // The top boundary is the highest amount committed by any *non-folded*
    // player. Everything between the last all-in level and this cap forms the
    // top pot, contested by players who still had chips behind. Folded players
    // never extend the eligible ceiling (they can only add dead money).
    let top_level = max_contender_committed(env, table)?;

    let mut prev_level: i128 = 0;
    for li in 0..levels.len() {
        let level = levels.get(li).ok_or(PokerTableError::InvalidPlayerIndex)?;
        if level <= prev_level || level > top_level {
            continue;
        }
        let pot = build_layer(env, table, prev_level, level)?;
        if pot.amount > 0 {
            pots.push_back(pot);
        }
        prev_level = level;
    }

    // Top layer: every remaining chip above the last all-in level. This sweeps
    // contributions up to the highest contender commitment plus any "dead money"
    // a folded player committed beyond that cap, so no chips are ever orphaned.
    // Eligibility is still capped at `top_level` (only players who reached the
    // top contender level can win it).
    let top = build_top_layer(env, table, prev_level, top_level)?;
    if top.amount > 0 {
        pots.push_back(top);
    }

    Ok(pots)
}

/// Collect the distinct total-commitment levels of non-folded all-in players,
/// sorted ascending. These are the boundaries between the main pot and side
/// pots. Folded players never create a boundary (they cannot win), though their
/// chips are still swept into whichever layers they reach.
fn distinct_all_in_levels(env: &Env, table: &TableState) -> Result<Vec<i128>, PokerTableError> {
    let mut levels: Vec<i128> = Vec::new(env);
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.folded || !p.all_in || p.committed <= 0 {
            continue;
        }
        insert_sorted_unique(&mut levels, p.committed);
    }
    Ok(levels)
}

/// Insert `value` into an ascending vector, keeping it sorted and de-duplicated.
fn insert_sorted_unique(levels: &mut Vec<i128>, value: i128) {
    for j in 0..levels.len() {
        let existing = levels.get(j).unwrap_or(0);
        if value == existing {
            return; // already present
        }
        if value < existing {
            levels.insert(j, value);
            return;
        }
    }
    levels.push_back(value);
}

/// Build a single pot layer covering contributions in the half-open band
/// `(lower, upper]`. Every player (including folded ones) contributes
/// `clamp(committed, lower, upper) - lower` chips to this layer. A non-folded
/// player is eligible to win it only if their total commitment reaches `upper`
/// (the top of the band), i.e. they fully covered this layer.
fn build_layer(
    env: &Env,
    table: &TableState,
    lower: i128,
    upper: i128,
) -> Result<SidePot, PokerTableError> {
    let mut amount: i128 = 0;
    let mut eligible: Vec<u32> = Vec::new(env);

    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.committed <= lower {
            continue;
        }
        let capped = core::cmp::min(p.committed, upper);
        amount += capped - lower;
        if !p.folded && p.committed >= upper {
            eligible.push_back(p.seat_index);
        }
    }

    Ok(SidePot {
        amount,
        eligible_players: eligible,
    })
}

/// The highest amount committed by any non-folded (contender) player. This is
/// the ceiling of the top pot's eligibility — only players who reached it can
/// win the final layer.
fn max_contender_committed(env: &Env, table: &TableState) -> Result<i128, PokerTableError> {
    let _ = env;
    let mut max = 0i128;
    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if !p.folded && p.committed > max {
            max = p.committed;
        }
    }
    Ok(max)
}

/// Build the final pot layer. Its amount sweeps every chip committed above
/// `lower` (including dead money from folded players who committed past the
/// contender ceiling), guaranteeing no chips are orphaned. Eligibility is
/// limited to non-folded players who committed up to `eligible_ceiling`.
fn build_top_layer(
    env: &Env,
    table: &TableState,
    lower: i128,
    eligible_ceiling: i128,
) -> Result<SidePot, PokerTableError> {
    let mut amount: i128 = 0;
    let mut eligible: Vec<u32> = Vec::new(env);

    for i in 0..table.players.len() {
        let p = table
            .players
            .get(i)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        if p.committed <= lower {
            continue;
        }
        amount += p.committed - lower;
        if !p.folded && p.committed >= eligible_ceiling && eligible_ceiling > lower {
            eligible.push_back(p.seat_index);
        }
    }

    Ok(SidePot {
        amount,
        eligible_players: eligible,
    })
}

/// Distribute every pot to its winner. `winner_seats` lists the seat indices of
/// the hand's ranked contenders best-first (as proved by the showdown circuit).
/// For each pot, the chips go to the highest-ranked contender that is eligible
/// for that pot (the best hand among players who contributed to it). Returns the
/// per-seat winnings credited so callers can emit a payout breakdown.
///
/// This guarantees the core invariant that a player can only win pots they have
/// contributed to, and that the total distributed equals the sum of the pots.
pub fn distribute_pots(
    env: &Env,
    table: &mut TableState,
    pots: &Vec<SidePot>,
    winner_seats: &Vec<u32>,
) -> Result<Vec<(u32, i128)>, PokerTableError> {
    let mut payouts: Vec<(u32, i128)> = Vec::new(env);

    for pi in 0..pots.len() {
        let pot = pots.get(pi).ok_or(PokerTableError::InvalidPlayerIndex)?;
        if pot.amount <= 0 {
            continue;
        }
        let winner_seat = best_eligible_winner(winner_seats, &pot.eligible_players)
            .ok_or(PokerTableError::WinnerNotEligibleForPot)?;

        let mut winner = table
            .players
            .get(winner_seat)
            .ok_or(PokerTableError::InvalidPlayerIndex)?;
        winner.stack += pot.amount;
        table.players.set(winner_seat, winner);

        accumulate_payout(&mut payouts, winner_seat, pot.amount);
    }

    Ok(payouts)
}

/// Find the best-ranked seat (earliest in `ranked`) that is eligible for a pot.
fn best_eligible_winner(ranked: &Vec<u32>, eligible: &Vec<u32>) -> Option<u32> {
    for ri in 0..ranked.len() {
        let seat = ranked.get(ri)?;
        for ei in 0..eligible.len() {
            if eligible.get(ei) == Some(seat) {
                return Some(seat);
            }
        }
    }
    None
}

/// Add `amount` to the running total for `seat`, merging into an existing entry
/// when a seat wins more than one pot.
fn accumulate_payout(payouts: &mut Vec<(u32, i128)>, seat: u32, amount: i128) {
    for i in 0..payouts.len() {
        if let Some((s, existing)) = payouts.get(i) {
            if s == seat {
                payouts.set(i, (seat, existing + amount));
                return;
            }
        }
    }
    payouts.push_back((seat, amount));
}

#[cfg(test)]
mod pot_test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Vec};

    /// Build a player with a given total commitment and all-in / folded flags.
    fn player(env: &Env, seat: u32, committed: i128, all_in: bool, folded: bool) -> PlayerState {
        PlayerState {
            address: Address::generate(env),
            stack: 0,
            bet_this_round: 0,
            committed,
            folded,
            all_in,
            sitting_out: false,
            seat_index: seat,
        }
    }

    /// Assemble a minimal TableState whose `pot` equals the sum of commitments.
    fn table_with(env: &Env, players: Vec<PlayerState>) -> TableState {
        let mut pot: i128 = 0;
        for i in 0..players.len() {
            pot += players.get(i).unwrap().committed;
        }
        let admin = Address::generate(env);
        TableState {
            id: 0,
            admin: admin.clone(),
            config: TableConfig {
                token: Address::generate(env),
                min_buy_in: 0,
                max_buy_in: i128::MAX,
                small_blind: 0,
                big_blind: 0,
                max_players: 9,
                timeout_ledgers: 0,
                committee: admin.clone(),
                verifier: admin.clone(),
                game_hub: admin.clone(),
            },
            phase: GamePhase::Showdown,
            players,
            dealer_seat: 0,
            current_turn: 0,
            pot,
            side_pots: Vec::new(env),
            deck_root: BytesN::from_array(env, &[0u8; 32]),
            hand_commitments: Vec::new(env),
            board_cards: Vec::new(env),
            dealt_indices: Vec::new(env),
            hand_number: 1,
            last_action_ledger: 0,
            committee: admin,
            session_id: 0,
        }
    }

    fn sum_pots(pots: &Vec<SidePot>) -> i128 {
        let mut total = 0i128;
        for i in 0..pots.len() {
            total += pots.get(i).unwrap().amount;
        }
        total
    }

    fn seats(env: &Env, ids: &[u32]) -> Vec<u32> {
        let mut v = Vec::new(env);
        for id in ids {
            v.push_back(*id);
        }
        v
    }

    // -----------------------------------------------------------------------
    // calculate_side_pots
    // -----------------------------------------------------------------------

    #[test]
    fn no_all_in_single_main_pot() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 100, false, false),
                player(&env, 1, 100, false, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        assert_eq!(pots.len(), 1);
        let main = pots.get(0).unwrap();
        assert_eq!(main.amount, 300);
        assert_eq!(main.eligible_players, seats(&env, &[0, 1, 2]));
    }

    #[test]
    fn three_way_unequal_all_ins() {
        // Classic case: stacks 50 / 100 / 100.
        // Main pot = 3*50 = 150 (all three eligible).
        // Side pot = 2*50 = 100 (seats 1 and 2 eligible).
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        assert_eq!(sum_pots(&pots), 250);
        assert_eq!(pots.len(), 2);

        let main = pots.get(0).unwrap();
        assert_eq!(main.amount, 150);
        assert_eq!(main.eligible_players, seats(&env, &[0, 1, 2]));

        let side = pots.get(1).unwrap();
        assert_eq!(side.amount, 100);
        assert_eq!(side.eligible_players, seats(&env, &[1, 2]));
    }

    #[test]
    fn four_way_three_distinct_all_in_levels() {
        // Commitments 20 / 50 / 80 / 80.
        // Layer (0,20]:  4*20 = 80   -> seats 0,1,2,3
        // Layer (20,50]: 3*30 = 90   -> seats 1,2,3
        // Layer (50,80]: 2*30 = 60   -> seats 2,3
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 20, true, false),
                player(&env, 1, 50, true, false),
                player(&env, 2, 80, true, false),
                player(&env, 3, 80, false, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        assert_eq!(sum_pots(&pots), 230);
        assert_eq!(pots.len(), 3);
        assert_eq!(pots.get(0).unwrap().amount, 80);
        assert_eq!(
            pots.get(0).unwrap().eligible_players,
            seats(&env, &[0, 1, 2, 3])
        );
        assert_eq!(pots.get(1).unwrap().amount, 90);
        assert_eq!(
            pots.get(1).unwrap().eligible_players,
            seats(&env, &[1, 2, 3])
        );
        assert_eq!(pots.get(2).unwrap().amount, 60);
        assert_eq!(pots.get(2).unwrap().eligible_players, seats(&env, &[2, 3]));
    }

    #[test]
    fn folded_player_is_dead_money_but_not_eligible() {
        // Seat 1 folded after committing 30. Seats 0 and 2 are all-in at the
        // SAME level (100), so there is a single pot; the folded 30 is dead
        // money swept into it, and only the two contenders are eligible.
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 100, true, false),
                player(&env, 1, 30, false, true), // folded
                player(&env, 2, 100, true, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        assert_eq!(sum_pots(&pots), 230);
        assert_eq!(pots.len(), 1);
        let p0 = pots.get(0).unwrap();
        assert_eq!(p0.amount, 230);
        assert_eq!(p0.eligible_players, seats(&env, &[0, 2]));
    }

    #[test]
    fn folded_dead_money_with_distinct_all_in_levels() {
        // Seat 1 folded after committing 80 (a large dead-money contribution).
        // Contenders: seat 0 all-in at 40, seat 2 all-in at 100.
        // Main pot (0,40]: 40+40+40 = 120 -> eligible 0,2
        // Side pot (40,100]: seat 1 adds 40 dead money + seat 2 adds 60 = 100
        //                    -> eligible 2 only (seat 0 capped at 40)
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 40, true, false),
                player(&env, 1, 80, false, true), // folded, dead money
                player(&env, 2, 100, true, false),
            ],
        );
        let table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        // No chips orphaned: 40 + 80 + 100 = 220.
        assert_eq!(sum_pots(&pots), 220);
        assert_eq!(pots.len(), 2);
        assert_eq!(pots.get(0).unwrap().amount, 120);
        assert_eq!(pots.get(0).unwrap().eligible_players, seats(&env, &[0, 2]));
        assert_eq!(pots.get(1).unwrap().amount, 100);
        assert_eq!(pots.get(1).unwrap().eligible_players, seats(&env, &[2]));
    }

    #[test]
    fn six_way_mixed_stacks() {
        // Commitments: 10, 25, 25, 60, 100, 100 (last two not all-in).
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 10, true, false),
                player(&env, 1, 25, true, false),
                player(&env, 2, 25, true, false),
                player(&env, 3, 60, true, false),
                player(&env, 4, 100, false, false),
                player(&env, 5, 100, false, false),
            ],
        );
        let table = table_with(&env, players.clone());
        let pots = calculate_side_pots(&env, &table).unwrap();

        // Total chips committed.
        let mut total = 0i128;
        for i in 0..players.len() {
            total += players.get(i).unwrap().committed;
        }
        assert_eq!(sum_pots(&pots), total);

        // Distinct all-in levels: 10, 25, 60, plus top layer at 100 -> 4 pots.
        assert_eq!(pots.len(), 4);
        // First (main) pot eligible to everyone.
        assert_eq!(
            pots.get(0).unwrap().eligible_players,
            seats(&env, &[0, 1, 2, 3, 4, 5])
        );
        // Top layer only seats 4 and 5.
        assert_eq!(pots.get(3).unwrap().eligible_players, seats(&env, &[4, 5]));
    }

    // -----------------------------------------------------------------------
    // distribute_pots
    // -----------------------------------------------------------------------

    #[test]
    fn distribute_short_stack_wins_only_main_pot() {
        // Seat 0 all-in 50 has the best hand; seats 1,2 cover 100.
        // Seat 0 wins the 150 main pot; seat 1 (next in ranking) wins the
        // 100 side pot it is eligible for.
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        // Ranking: seat 0 best, then 1, then 2.
        let ranking = seats(&env, &[0, 1, 2]);
        let payouts = distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        assert_eq!(table.players.get(0).unwrap().stack, 150); // main pot
        assert_eq!(table.players.get(1).unwrap().stack, 100); // side pot
        assert_eq!(table.players.get(2).unwrap().stack, 0);

        // Conservation: everything distributed.
        let mut paid = 0i128;
        for i in 0..payouts.len() {
            paid += payouts.get(i).unwrap().1;
        }
        assert_eq!(paid, 250);
    }

    #[test]
    fn distribute_big_stack_wins_everything() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 50, true, false),
                player(&env, 1, 100, true, false),
                player(&env, 2, 100, false, false),
            ],
        );
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();

        // Seat 2 has the best hand and contributed to every pot -> wins all 250.
        let ranking = seats(&env, &[2, 1, 0]);
        distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        assert_eq!(table.players.get(2).unwrap().stack, 250);
        assert_eq!(table.players.get(0).unwrap().stack, 0);
        assert_eq!(table.players.get(1).unwrap().stack, 0);
    }

    #[test]
    fn distribute_conserves_all_chips_six_way() {
        let env = Env::default();
        let players = Vec::from_array(
            &env,
            [
                player(&env, 0, 10, true, false),
                player(&env, 1, 25, true, false),
                player(&env, 2, 25, true, false),
                player(&env, 3, 60, true, false),
                player(&env, 4, 100, false, false),
                player(&env, 5, 100, false, false),
            ],
        );
        let total: i128 = 10 + 25 + 25 + 60 + 100 + 100;
        let mut table = table_with(&env, players);
        let pots = calculate_side_pots(&env, &table).unwrap();
        let ranking = seats(&env, &[3, 5, 4, 2, 1, 0]);
        distribute_pots(&env, &mut table, &pots, &ranking).unwrap();

        let mut paid = 0i128;
        for i in 0..table.players.len() {
            paid += table.players.get(i).unwrap().stack;
        }
        assert_eq!(paid, total);
    }
}
