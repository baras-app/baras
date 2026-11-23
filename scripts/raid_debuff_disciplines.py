#!/usr/bin/env python3
"""Regenerate core/data/raid_debuff_disciplines.csv: which disciplines apply each raid debuff.

The game data only says which abilities carry a debuff and which disciplines have the
ability; whether a discipline's version applies it depends on talents the data does not
resolve (every Powertech has Rocket Punch, only Advanced Prototype's sunders). So a pair is
kept when the game data allows it AND it shows up in real logs: at least MIN_APPLICATIONS
applications by players of that discipline or its mirror.

Usage: scripts/raid_debuff_disciplines.py <dir of combat logs>   (run from the repo root)
"""

import collections
import csv
import glob
import re
import sys

MIN_APPLICATIONS = 20
APPLY_EFFECT = "ApplyEffect {836045448945477}"
# Game tag -> RaidDebuff variant (same table as core/build.rs)
TAGS = {
    "debuff.armor_reduced_stacks_by_tag": "Sundered",
    "debuff.ranged_vulnerability": "Marked",
    "debuff.melee_vulnerability": "BeatDown",
    "debuff.tech_vulnerability": "Susceptible",
    "debuff.force_vulnerability": "Vulnerable",
    "debuff.elemental_and_internal_vulnerability": "Assailable",
    "debuff.aoe_vulnerability": "Overwhelmed",
    "debuff.trauma": "Trauma",
    "debuff.tech_and_force_damage_reduced": "Impaired",
    "debuff.ranged_and_melee_damage_reduced": "Weakened",
    "debuff.accuracy_reduced_stacks_by_tag": "Unsteady",
}
MIRRORS = [
    ("assassin.darkness", "shadow.combat"),
    ("assassin.deception", "shadow.infiltration"),
    ("assassin.hatred", "shadow.serenity"),
    ("sorcerer.lightning", "sage.telekinetics"),
    ("sorcerer.madness", "sage.balance"),
    ("sorcerer.corruption", "sage.seer"),
    ("juggernaut.immortal", "guardian.defense"),
    ("juggernaut.vengeance", "guardian.vigilance"),
    ("juggernaut.rage", "guardian.focus"),
    ("marauder.annihilation", "sentinel.watchman"),
    ("marauder.carnage", "sentinel.combat"),
    ("marauder.fury", "sentinel.concentration"),
    ("mercenary.arsenal", "commando.gunnery"),
    ("mercenary.innovative_ordnance", "commando.assault_specialist"),
    ("mercenary.bodyguard", "commando.combat_medic"),
    ("powertech.shield_tech", "specialist.shield_specialist"),
    ("powertech.firebug", "specialist.plasmatech"),
    ("powertech.advanced_prototype", "specialist.tactics"),
    ("operative.concealment", "scoundrel.scrapper"),
    ("operative.lethality", "scoundrel.ruffian"),
    ("operative.medic", "scoundrel.sawbones"),
    ("sniper.marksmanship", "gunslinger.sharpshooter"),
    ("sniper.engineering", "gunslinger.saboteur"),
    ("sniper.virulence", "gunslinger.dirty_fighting"),
]


def rows(path):
    with open(path, encoding="utf-8-sig", newline="") as f:
        yield from csv.DictReader(f)


def main(log_dir):
    pair_of = {key: pair for pair in MIRRORS for key in pair}
    guid_key = {r["guid"]: r["discipline"] for r in rows("core/data/discipline_ids.csv")}
    disciplines = {r["fqn"]: r["disciplines"].split(";") for r in rows("core/data/ability_data.csv")}

    effect_debuff = {}
    allowed = set()  # (mirror pair, debuff) the game data allows
    for r in rows("core/data/effect_data.csv"):
        debuff = next((TAGS[t] for t in r["tags"].split(";") if t in TAGS), None)
        if debuff and r["has_own_name"] == "True":
            effect_debuff[r["effect_id"]] = debuff
            allowed |= {(pair_of[d], debuff) for d in disciplines.get(r["ability_fqn"], []) if d in pair_of}

    source = re.compile(r"^\[[\d:.]+\] \[(@[^|\]/]*)[|\]]")
    applied = collections.Counter()
    for path in glob.glob(f"{log_dir}/**/*.txt", recursive=True):
        discipline = {}  # player -> discipline key
        with open(path, encoding="cp1252", errors="replace") as f:
            for line in f:
                if "DisciplineChanged" in line:
                    who = source.match(line)
                    guids = re.findall(r"\{(\d+)\}", line.split("DisciplineChanged")[1])
                    if who and guids and guids[-1] in guid_key:
                        discipline[who.group(1)] = guid_key[guids[-1]]
                elif APPLY_EFFECT in line:
                    effect = re.search(r"ApplyEffect \{\d+\}: [^{]*\{(\d+)\}", line)
                    who = source.match(line)
                    if effect and who and effect.group(1) in effect_debuff and who.group(1) in discipline:
                        applied[(pair_of[discipline[who.group(1)]], effect_debuff[effect.group(1)])] += 1

    kept = sorted(
        (debuff, key)
        for (pair, debuff), count in applied.items()
        if count >= MIN_APPLICATIONS and (pair, debuff) in allowed
        for key in pair
    )
    order = list(TAGS.values())
    kept.sort(key=lambda r: (order.index(r[0]), r[1]))
    with open("core/data/raid_debuff_disciplines.csv", "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["debuff", "discipline"])
        writer.writerows(kept)
    print(f"{len(kept)} rows from {len(applied)} observed pairs")


if __name__ == "__main__":
    main(sys.argv[1])
