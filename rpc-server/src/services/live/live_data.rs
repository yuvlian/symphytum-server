use database::Database;
use resource::master::MasterTable;
use sqlx::Error;
use types::entity::master::{
    Card, CardLevel, CardLevelLimit, CardPotential, Costume, LiveDeckEvaluationRank,
    LiveDeckPowerRank, LiveLeaderSkill, LivePassiveSkillEffect, LiveScoreEvaluationRank, Music,
};
use types::enums::{
    CardPotentialEffectType, LiveDeckRankType, LiveEvaluationRankType, LivePassiveSkillEffectType,
};
use types::rpc::api::common::live_deck_evaluation;
use types::rpc::api::common::live_deck_in_game_effect;
use types::rpc::api::common::{LiveDeckEvaluation, LiveDeckInGameEffect, LiveDeckPosition};

/// max-level base parameter value for a card (its CardLevel group's top level)
pub fn card_power(card: &Card) -> i64 {
    CardLevel::table()
        .iter()
        .filter(|l| l.group_id == card.card_level_group_id)
        .max_by_key(|l| l.level)
        .map(|l| l.parameter_base_value)
        .unwrap_or(0)
}

pub fn card_stats(card: &Card) -> (i64, i64, i64) {
    (
        card.performance_permil_multiply as i64,
        card.technique_permil_multiply as i64,
        card.sense_permil_multiply as i64,
    )
}

/// highest CardLevel[group] whose exp threshold is met, capped by the limit-break level limit (rows sorted by level first).
pub fn level_of(card: &Card, exp: i64, limit_break_count: i32) -> i32 {
    let mut rows: Vec<&CardLevel> = CardLevel::table()
        .iter()
        .filter(|l| l.group_id == card.card_level_group_id)
        .collect();
    rows.sort_by_key(|l| l.level);
    let mut level = 1;
    for l in rows {
        if l.level > 1 && l.exp > 0 && l.exp <= exp {
            level = level.max(l.level);
        }
    }
    let limit = CardLevelLimit::table()
        .iter()
        .filter(|l| {
            l.group_id == card.card_level_limit_group_id && l.limit_break_count == limit_break_count
        })
        .map(|l| l.level_limit)
        .max()
        .unwrap_or(i32::MAX);
    level.min(limit)
}

/// base parameter value at a card's level (mirrors card/card_data.rs).
pub fn parameter_base(card: &Card, level: i32) -> i64 {
    CardLevel::table()
        .iter()
        .find(|l| l.group_id == card.card_level_group_id && l.level == level)
        .map(|l| l.parameter_base_value)
        .unwrap_or(0)
}

/// (all-parameter-up permil, connect-effect level) from potentials at or below the upgrade count.
pub fn potential_bonus(card: &Card, upgrade_count: i32) -> (i64, i32) {
    let mut bonus = 0i64;
    let mut connect = 1i32;
    for p in CardPotential::table()
        .iter()
        .filter(|p| p.group_id == card.card_potential_group_id && p.upgrade_count <= upgrade_count)
    {
        match CardPotentialEffectType::try_from(p.effect_type).unwrap_or_default() {
            CardPotentialEffectType::AllParameterUpPermilUp => bonus += p.value,
            CardPotentialEffectType::SkillTreeConnectEffectLevelUp => {
                connect = connect.max(p.value as i32);
            }
            _ => {}
        }
    }
    (bonus, connect)
}

/// ceil(permil * parameter/1000 * (1 + bonusPermil/1000)), mirrors the client.
pub fn attribute_value(parameter: i64, permil_multiply: i32, bonus_permil: i64) -> i64 {
    let v =
        permil_multiply as f32 * (parameter as f32 / 1000.0) * (1.0 + bonus_permil as f32 / 1000.0);
    v.ceil() as i64
}

/// outfit skill: costume -> LiveLeaderSkill -> LivePassiveSkillEffect; -all effects add per member, others hit the leader only.
pub fn outfit_skill_up(deck_costume_id: &str, members: &[(&Card, i64)]) -> i64 {
    let Some(costume) = Costume::table().iter().find(|c| c.id == deck_costume_id) else {
        return 0;
    };
    let Some(leader) = LiveLeaderSkill::table()
        .iter()
        .find(|l| l.id == costume.live_leader_skill_id)
    else {
        return 0;
    };
    let effects: Vec<&LivePassiveSkillEffect> = LivePassiveSkillEffect::table()
        .iter()
        .filter(|e| e.group_id == leader.live_passive_skill_effect_group_id)
        .collect();
    if effects.is_empty() {
        return 0;
    }
    let mut total = 0i64;
    for (i, (card, base)) in members.iter().enumerate() {
        for e in &effects {
            if e.r#type != LivePassiveSkillEffectType::PerformanceUpPermilUp as i32 {
                continue;
            }
            if !e.live_skill_effect_target_id.ends_with("-all") && i != 0 {
                continue; // non-all targets hit the leader (position 1) only
            }
            let v = (*base as f64)
                * (card.performance_permil_multiply as f64 / 1000.0)
                * (e.value as f64 / 1000.0);
            total += v.ceil() as i64;
        }
    }
    total
}

pub fn deck_power_rank(power: i64) -> (i32, i32) {
    // highest LiveDeckRankType whose threshold the power clears; plus within type
    let mut best = (LiveDeckRankType::Unknown as i32, 0i32);
    for row in LiveDeckPowerRank::table() {
        if power >= row.threshold {
            let t = row.r#type;
            if t > best.0 || (t == best.0 && row.plus > best.1) {
                best = (t, row.plus);
            }
        }
    }
    best
}

/// evaluation rank (LiveDeckEvaluationRank master, separate from the power rank); D row is the fallback.
pub fn evaluation_rank(value: i64) -> (i32, i32) {
    let mut best = (LiveDeckRankType::D as i32, 0i32);
    for row in LiveDeckEvaluationRank::table() {
        if value >= row.threshold {
            let t = row.r#type;
            if t > best.0 || (t == best.0 && row.plus > best.1) {
                best = (t, row.plus);
            }
        }
    }
    best
}

/// multiplier for the final unit score (tuning constant).
pub const FINAL_SCORE_MULTIPLIER: i64 = 6;

/// deck power = Σ per-card powers + outfit up; unit score = 5 * member
/// parameter * FINAL_SCORE_MULTIPLIER; the passive/poster/board and
/// score-permil components stay 0 (real values 2513/760/750/381/117).
pub fn evaluation_for(
    power_rows: &[(i32, String, i64)],
    member_parameter: i64,
    outfit_up: i64,
) -> Option<LiveDeckEvaluation> {
    let base_power: i64 = power_rows.iter().map(|(_, _, p)| p).sum();
    let live_deck_power = base_power + outfit_up;
    let evaluation_value = FINAL_SCORE_MULTIPLIER * 5 * member_parameter;
    let (eval_rank, eval_plus) = evaluation_rank(evaluation_value);
    let (pow_rank, pow_plus) = deck_power_rank(live_deck_power);
    Some(LiveDeckEvaluation {
        live_deck_evaluation_value: evaluation_value,
        live_deck_evaluation_rank_type: eval_rank,
        live_deck_evaluation_rank_plus_value: eval_plus,
        live_deck_power: Some(live_deck_evaluation::LiveDeckPower {
            live_deck_power,
            live_deck_power_rank_type: pow_rank,
            live_deck_power_rank_plus_value: pow_plus,
            // member parameter = Σ per-card base parameters at current levels (misleading proto name)
            card_potential_parameter_evaluation_value: member_parameter,
            card_parameter_up_by_live_leader_skill: outfit_up,
            card_parameter_up_by_live_passive_skill: 0, // real 2513
            card_parameter_up_by_skill_tree: 0,         // real 750
            card_parameter_up_by_poster_collect: 0,     // real 760
            card_training_evaluation_value: 0,
        }),
        live_deck_evaluation_score_up_permil_up: Some(
            live_deck_evaluation::LiveDeckEvaluationScoreUpPermilUp {
                score_up_permil_up: 0,
                score_up_permil_up_rank_type: 0,
                score_up_permil_up_rank_plus_value: 0,
                score_up_permil_up_by_live_active_skill: 0, // real 381
                score_up_permil_up_by_live_leader_skill: 0,
                score_up_permil_up_by_live_passive_skill: 0,
                score_up_permil_up_by_skill_tree: 0,
                score_up_permil_up_by_live_special_skill: 0, // real 117
            },
        ),
    })
}

/// positions -> (position, card_id, power, performance, technique, sense)
pub fn resolve_positions(card_ids: &[(i32, String)]) -> Vec<(i32, String, i64, i64, i64, i64)> {
    card_ids
        .iter()
        .filter_map(|(pos, card_id)| {
            Card::table().iter().find(|c| c.id == *card_id).map(|card| {
                let (perf, tech, sense) = card_stats(card);
                (*pos, card.id.clone(), card_power(card), perf, tech, sense)
            })
        })
        .collect()
}

/// like resolve_positions, but levels come from user_cards and each row carries the base parameter.
pub async fn resolve_positions_for_user(
    db: &Database,
    uid: &str,
    card_ids: &[(i32, String)],
) -> Result<Vec<(i32, String, i64, i64, i64, i64, i64)>, Error> {
    let user_cards = db.user_cards(uid).await?;
    let mut out = Vec::with_capacity(card_ids.len());
    for (pos, card_id) in card_ids {
        let Some(card) = Card::table().iter().find(|c| &c.id == card_id) else {
            continue;
        };
        let Some(uc) = user_cards.iter().find(|uc| uc.card_id == *card_id) else {
            continue;
        };
        let level = level_of(card, uc.exp, uc.level_limit_break_count);
        let base = parameter_base(card, level);
        let (bonus, _) = potential_bonus(card, uc.potential_upgrade_count);
        let performance = attribute_value(base, card.performance_permil_multiply, bonus);
        let technique = attribute_value(base, card.technique_permil_multiply, bonus);
        let sense = attribute_value(base, card.sense_permil_multiply, bonus);
        out.push((
            *pos,
            card.id.clone(),
            performance + technique + sense,
            performance,
            technique,
            sense,
            base,
        ));
    }
    Ok(out)
}

pub fn in_game_effect(
    positions: &[(i32, String, i64, i64, i64, i64)],
) -> Option<LiveDeckInGameEffect> {
    Some(LiveDeckInGameEffect {
        life_up: 0,
        // the client's LiveScoreCalculator scales the live score by
        // (1 + live_score_bonus_permil_up_by_skill_tree / 1000); this is the
        // server-side knob for the in-game score (the recorded score stays
        // untouched).
        live_score_bonus_permil_up_by_skill_tree: (FINAL_SCORE_MULTIPLIER - 1) as f32 * 1000.0,
        live_deck_positions: positions
            .iter()
            .map(|(pos, card_id, _, perf, tech, sense)| {
                live_deck_in_game_effect::LiveDeckPosition {
                    position: *pos,
                    card_id: card_id.clone(),
                    performance: *perf,
                    technique: *tech,
                    sense: *sense,
                    ..Default::default()
                }
            })
            .collect(),
        ..Default::default()
    })
}

pub fn deck_positions(positions: &[(i32, String, i64, i64, i64, i64)]) -> Vec<LiveDeckPosition> {
    positions
        .iter()
        .map(|(pos, card_id, power, _, _, _)| LiveDeckPosition {
            position: *pos,
            card_id: card_id.clone(),
            live_deck_power: *power * (FINAL_SCORE_MULTIPLIER + 3),
        })
        .collect()
}

/// highest score-evaluation rank whose threshold the score meets; D row is the fallback, plus rows win ties.
pub fn score_evaluation_rank(music_id: &str, score: i64) -> (LiveEvaluationRankType, i32) {
    let Some(music) = Music::table().iter().find(|m| m.id == music_id) else {
        return (LiveEvaluationRankType::D, 0);
    };
    let mut best: Option<(&LiveScoreEvaluationRank, i32)> = None;
    for r in LiveScoreEvaluationRank::table()
        .iter()
        .filter(|r| r.group_id == music.single_live_score_evaluation_rank_group_id && r.score > 0)
    {
        if r.score <= score
            && best
                .map(|(b, _)| {
                    (r.evaluation_rank_type as i32, r.plus)
                        > (b.evaluation_rank_type as i32, b.plus)
                })
                .unwrap_or(true)
        {
            best = Some((r, r.evaluation_rank_type as i32));
        }
    }
    match best {
        Some((r, rank)) => (
            LiveEvaluationRankType::try_from(rank).unwrap_or(LiveEvaluationRankType::D),
            r.plus,
        ),
        None => (LiveEvaluationRankType::D, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::Database;

    const TEAM: [&str; 5] = [
        "card-00007-5-uniq-0008-00",
        "card-00023-3-nrml-0019-00",
        "card-00034-4-cmmn-0000-00",
        "card-04017-3-nrml-0056-00",
        "card-00039-3-nrml-0032-00",
    ];

    #[tokio::test]
    async fn user_team_unit_score_matches_ground_truth() {
        // real deck: level 19 (exp 10013), potential 0; real server: unit 188458, power 61750, C+0
        let dir = std::env::temp_dir().join(format!("symlive-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let db = Database::open(&dir.join("test.db")).await.expect("open db");
        let uid = db
            .get_or_create_account("live-score-test")
            .await
            .expect("account");
        for c in TEAM {
            sqlx::query(
                "UPDATE user_cards SET exp = 10013, level_limit_break_count = 0, potential_upgrade_count = 0 WHERE account_id = ? AND card_id = ?",
            )
            .bind(&uid)
            .bind(c)
            .execute(db.pool())
            .await
            .expect("reset card");
        }

        resource::master::load::<Card>().await.expect("card");
        resource::master::load::<CardLevel>().await.expect("level");
        resource::master::load::<CardLevelLimit>()
            .await
            .expect("limit");
        resource::master::load::<CardPotential>()
            .await
            .expect("potential");
        resource::master::load::<Costume>().await.expect("costume");
        resource::master::load::<LiveLeaderSkill>()
            .await
            .expect("leader");
        resource::master::load::<LivePassiveSkillEffect>()
            .await
            .expect("effect");
        resource::master::load::<LiveDeckEvaluationRank>()
            .await
            .expect("eval rank");
        resource::master::load::<LiveDeckPowerRank>()
            .await
            .expect("power rank");

        let positions: Vec<(i32, String)> = TEAM
            .iter()
            .enumerate()
            .map(|(i, c)| ((i + 1) as i32, c.to_string()))
            .collect();
        let resolved = resolve_positions_for_user(&db, &uid, &positions)
            .await
            .expect("resolve");
        assert_eq!(resolved.len(), 5);
        let member_parameter: i64 = resolved.iter().map(|r| r.6).sum();
        assert_eq!(member_parameter, 37598, "member parameter = Σ base@19");

        let members: Vec<(&Card, i64)> = resolved
            .iter()
            .filter_map(|(_, cid, _, _, _, _, base)| {
                Card::table()
                    .iter()
                    .find(|c| &c.id == cid)
                    .map(|c| (c, *base))
            })
            .collect();
        let outfit = outfit_skill_up("cos-00007-uniq-0008-00", &members);
        assert_eq!(outfit, 20115, "leader +1200 permil performance to all");

        let power_rows: Vec<(i32, String, i64)> = resolved
            .iter()
            .map(|(p, c, pw, _, _, _, _)| (*p, c.clone(), *pw))
            .collect();
        let eval = evaluation_for(&power_rows, member_parameter, outfit).expect("eval");
        assert_eq!(
            eval.live_deck_evaluation_value,
            5 * 37_598 * FINAL_SCORE_MULTIPLIER,
            "5 * member parameter * FINAL_SCORE_MULTIPLIER"
        );
        let lp = eval.live_deck_power.as_ref().expect("power block");
        // per-card powers are ceil(basexpermil/1000) sums ≈ base (permils sum to 1000)
        assert!(
            (lp.live_deck_power - (37598 + 20115)).abs() <= 10,
            "deck power ≈ member + outfit, got {}",
            lp.live_deck_power
        );
        // the evaluation rank is derived from the sent value (band shifts with
        // the FINAL_SCORE_MULTIPLIER tuning)
        let (rank, plus) = evaluation_rank(eval.live_deck_evaluation_value);
        assert_eq!(eval.live_deck_evaluation_rank_type, rank);
        assert_eq!(eval.live_deck_evaluation_rank_plus_value, plus);
        assert_eq!(
            lp.live_deck_power_rank_type,
            LiveDeckRankType::C as i32,
            "power rank C (real: C+0)"
        );
        assert_eq!(lp.live_deck_power_rank_plus_value, 0);
    }
}
