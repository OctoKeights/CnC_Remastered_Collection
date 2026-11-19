#!/usr/bin/env python3
"""Generates Tiberian Dawn's built-in type data from the original sources.

Tiberian Dawn has no rules file: every object's stats are constructor arguments in the
*DATA.CPP files, and weapons and warheads are tables in CONST.CPP. This script:

* runs a small C preprocessor over the files, with the symbols DEFINES.H defines (so
  `#ifdef ADVANCED` / `#ifdef PATCH` blocks resolve as the shipped game built them);
* reads each `XxxTypeClass const Name(...)` definition and pairs its arguments with the
  constructor's parameter names;
* evaluates the C expressions (enum values, `STRUCTF_*` / `HOUSEF_*` bit masks,
  `XYP_COORD`, `XYCELL`, the foundation cell lists, ...) and writes typed Rust.

Usage (from the repository root): python3 rust/tools/gen_td_data.py
Rewrites rust/crates/cnc-td/src/data/{enums.rs,objects.rs} (run `cargo fmt` after).
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TD = ROOT / "TIBERIANDAWN"
OUT = ROOT / "rust" / "crates" / "cnc-td" / "src" / "data"


def read(name):
    return (TD / name).read_text(encoding="latin-1")


# ---------------------------------------------------------------------------------------
# Preprocessing.

def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", lambda m: "\n" * m.group(0).count("\n"), text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def preprocess(text, defined):
    """Resolves #if/#ifdef/#ifndef/#else/#endif; records #define while active."""
    out, stack = [], []  # stack of (active, taken)
    active = True
    for line in strip_comments(text).splitlines():
        s = line.strip()
        m = re.match(r"#\s*(ifdef|ifndef|if|elif|else|endif|define)\b\s*(.*)", s)
        if not m:
            if active:
                out.append(line)
            continue
        kind, rest = m.group(1), m.group(2).strip()
        if kind in ("ifdef", "ifndef", "if"):
            if kind == "ifdef":
                cond = rest.split()[0] in defined
            elif kind == "ifndef":
                cond = rest.split()[0] not in defined
            else:
                cond = eval_condition(rest, defined)
            stack.append((active, cond))
            active = active and cond
        elif kind == "elif":
            parent, taken = stack.pop()
            cond = not taken and eval_condition(rest, defined)
            stack.append((parent, taken or cond))
            active = parent and cond
        elif kind == "else":
            parent, taken = stack.pop()
            stack.append((parent, True))
            active = parent and not taken
        elif kind == "endif":
            parent, _ = stack.pop()
            active = parent
        elif kind == "define" and active:
            name = rest.split("(")[0].split()[0] if rest else ""
            if name:
                defined.add(name)
            out.append(line)
    return "\n".join(out)


def eval_condition(expr, defined):
    e = re.sub(r"defined\s*\(\s*(\w+)\s*\)", lambda m: "1" if m.group(1) in defined else "0", expr)
    e = re.sub(r"defined\s+(\w+)", lambda m: "1" if m.group(1) in defined else "0", e)
    e = e.replace("&&", " and ").replace("||", " or ").replace("!", " not ")
    e = re.sub(r"\b[A-Za-z_]\w*\b", lambda m: m.group(0) if m.group(0) in ("and", "or", "not") else "0", e)
    try:
        return bool(eval(e))
    except Exception:
        sys.exit(f"cannot evaluate #if {expr}")


defined = {"WIN32", "_WIN32", "REMASTER_BUILD"}
defines_h = preprocess(read("DEFINES.H"), defined)

# ---------------------------------------------------------------------------------------
# Enums and constants.

CONSTS = {"MAP_CELL_W": 64, "MCW": 64, "MAP_CELL_H": 64, "REFRESH_EOL": 32767, "CELL_PIXEL_W": 24,
          "CELL_PIXEL_H": 24, "ICON_PIXEL_W": 24, "ICON_PIXEL_H": 24, "ICON_LEPTON_W": 256,
          "ICON_LEPTON_H": 256, "CELL_LEPTON_W": 256, "CELL_LEPTON_H": 256, "true": 1, "false": 0}


def c_div(a, b):
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def evaluate(expr):
    e = " ".join(expr.split())
    e = re.sub(r"\((?:short const \*|unsigned long|unsigned|long|int|short|COORDINATE|DirType|MPHType)\)", "", e)
    e = re.sub(r"(\d+)[lLuU]+\b", r"\1", e)
    e = e.replace("||", " or ").replace("&&", " and ")
    py = re.sub(r"\b[A-Za-z_]\w*\b", lambda m: m.group(0) if m.group(0) in ("XYP_COORD", "XYCELL", "c_div", "or", "and") else f"_c[{m.group(0)!r}]", e)
    py = py.replace("/", "//")
    try:
        return eval(py, {"_c": CONSTS, "XYP_COORD": xyp_coord, "XYCELL": lambda x, y: y * 64 + x, "c_div": c_div})
    except KeyError as k:
        raise ValueError(f"unknown constant {k} in {expr!r}")


def xyp_coord(x, y):
    # (unsigned long)(((x)*ICON_LEPTON_W)/CELL_PIXEL_W + ((((y)*ICON_LEPTON_H)/CELL_PIXEL_H)<<16))
    return (c_div(x * 256, 24) + (c_div(y * 256, 24) << 16)) & 0xFFFFFFFF


def parse_enum(cname):
    m = re.search(r"typedef\s+enum\s+" + cname + r"\b[^{]*\{(.*?)\}\s*" + cname + r"\s*;", defines_h, re.S)
    if not m:
        sys.exit(f"enum {cname} not found")
    value, variants = -1, []
    for part in m.group(1).split(","):
        part = part.strip()
        if not part:
            continue
        if "=" in part:
            ident, expr = (p.strip() for p in part.split("=", 1))
            value = evaluate(expr)
        else:
            ident = part
            value += 1
        CONSTS[ident] = value
        variants.append((ident, value))
    return variants


ENUMS = {}
for cname in ["HousesType", "StructType", "UnitType", "InfantryType", "AircraftType", "WeaponType",
              "WarheadType", "BulletType", "ArmorType", "SpeedType", "AnimType", "VocType", "RTTIType",
              "BSizeType", "DirType", "MissionType", "MPHType", "TheaterType", "LandType", "TemplateType",
              "OverlayType", "TerrainType"]:
    ENUMS[cname] = parse_enum(cname)
for ident, value in ENUMS["StructType"]:
    if value >= 0:
        CONSTS["STRUCTF_" + ident[len("STRUCT_"):]] = 1 << value
CONSTS["STRUCTF_NONE"] = 0
for ident, value in ENUMS["HousesType"]:
    if value >= 0:
        CONSTS["HOUSEF_" + ident[len("HOUSE_"):]] = 1 << value
for name, expr in re.findall(r"#define\s+(HOUSEF_\w+|STRUCTF_\w+|THEATERF_\w+)\s+(.+)", defines_h):
    try:
        CONSTS[name] = evaluate(expr)
    except ValueError:
        pass


def rust_ident(ident, prefix):
    stem = ident[len(prefix):] if ident.startswith(prefix) else ident
    name = "".join(p[:1].upper() + p[1:].lower() for p in stem.split("_") if p)
    return ("T" + name) if name[:1].isdigit() else name


def real_variants(cname, prefix):
    """Variants worth a Rust name: skip NONE/COUNT/FIRST and aliases of earlier values."""
    seen, out = set(), []
    count = dict(ENUMS[cname]).get(prefix + "COUNT")
    for ident, value in ENUMS[cname]:
        stem = ident[len(prefix):]
        # `TEMPLATE_NONE = 255` sits past the count: a sentinel, not a type.
        past_end = count is not None and value >= count
        if value < 0 or past_end or stem in ("COUNT", "FIRST", "MIN", "MAX") or value in seen:
            continue
        seen.add(value)
        out.append((ident, value))
    return out


# New Rust enums this file defines (the object type enums already exist in types.rs).
NEW_ENUMS = [
    ("Weapon", "WeaponType", "WEAPON_", "A weapon (`WeaponType`)."),
    ("Warhead", "WarheadType", "WARHEAD_", "A warhead (`WarheadType`)."),
    ("Bullet", "BulletType", "BULLET_", "A projectile (`BulletType`)."),
    ("Armor", "ArmorType", "ARMOR_", "An armour class (`ArmorType`)."),
    ("Locomotion", "SpeedType", "SPEED_", "How an object moves (`SpeedType`)."),
    ("Anim", "AnimType", "ANIM_", "An animation (`AnimType`)."),
    ("Voc", "VocType", "VOC_", "A sound effect (`VocType`)."),
    ("Rtti", "RTTIType", "RTTI_", "A kind of game object (`RTTIType`)."),
    ("BuildingSize", "BSizeType", "BSIZE_", "A building footprint (`BSizeType`)."),
    ("Land", "LandType", "LAND_", "A kind of ground (`LandType`)."),
]

enums_rs = [
    "//! Enumerations used by Tiberian Dawn's built-in type data, with the original numeric",
    "//! values.",
    "//!",
    "//! @generated by `rust/tools/gen_td_data.py` from `DEFINES.H`. Do not edit by hand.",
    "",
]
for rust, cname, prefix, doc in NEW_ENUMS:
    variants = real_variants(cname, prefix)
    repr_ty = "u8" if max(v for _, v in variants) < 256 else "u16"
    enums_rs += [f"/// {doc}", "#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]",
                 f"#[repr({repr_ty})]", f"pub enum {rust} {{"]
    for ident, value in variants:
        enums_rs += [f"    /// `{ident}`.", f"    {rust_ident(ident, prefix)} = {value},"]
    enums_rs += ["}", "", f"impl {rust} {{", "    /// Every variant, in id order.",
                 f"    pub const ALL: &'static [Self] = &[" + ", ".join(f"Self::{rust_ident(i, prefix)}" for i, _ in variants) + "];",
                 "", "    /// Looks up a variant by its original numeric value.", "    #[must_use]", "    #[allow(clippy::too_many_lines)]",
                 f"    pub const fn from_id(id: {repr_ty}) -> Option<Self> {{", "        match id {"]
    for ident, value in variants:
        enums_rs.append(f"            {value} => Some(Self::{rust_ident(ident, prefix)}),")
    enums_rs += ["            _ => None,", "        }", "    }", "", "    /// The original numeric value.",
                 "    #[must_use]", f"    pub const fn id(self) -> {repr_ty} {{", f"        self as {repr_ty}", "    }", "}", ""]

# ---------------------------------------------------------------------------------------
# Value conversion.

existing_types = (ROOT / "rust" / "crates" / "cnc-td" / "src" / "types.rs").read_text()
scenario_rs = (ROOT / "rust" / "crates" / "cnc-td" / "src" / "scenario.rs").read_text()
mission_by_id = {int(v): n for n, v in re.findall(r"(\w+) = (\d+) => \"[^\"]+\"", scenario_rs.split("pub enum Mission")[1].split("}")[0])}


def existing_variant(enum, cname, prefix, ident):
    value = dict(ENUMS[cname])[ident]
    body = existing_types.split(f"pub enum {enum}:")[1].split("\n    }")[0]
    for name, v in re.findall(r"(\w+) = (\d+) =>", body):
        if int(v) == value:
            return f"{enum}::{name}"
    raise ValueError(f"no {enum} variant with id {value} ({ident})")


def enum_value(rust, cname, prefix, expr, optional):
    ident = expr.strip()
    stem = ident[len(prefix):]
    count = dict(ENUMS[cname]).get(prefix + "COUNT")
    if evaluate(ident) < 0 or (count is not None and evaluate(ident) >= count):
        if not optional:
            raise ValueError(f"{ident} is not allowed here")
        return "None"
    value = evaluate(ident)
    for i, v in ENUMS[cname]:
        if v == value and i[len(prefix):] not in ("COUNT", "FIRST", "MIN", "MAX"):
            name = rust_ident(i, prefix) if rust not in ("Mission",) else mission_by_id[value]
            break
    else:
        raise ValueError(f"{ident} has no variant")
    # Use the first variant with this value (aliases collapse to it).
    for i, v in real_variants(cname, prefix) if rust != "Mission" else []:
        if v == value:
            name = rust_ident(i, prefix)
            break
    text = f"{rust}::{name}"
    return f"Some({text})" if optional else text


# C++ parameter type -> (Rust type, converter)
def convert(ctype, pname, expr, lists):
    ctype = " ".join(ctype.split())
    if pname == "theater":
        return "u8", f"0x{evaluate(expr):02X}"
    if ctype == "bool":
        return "bool", {"1": "true", "0": "false"}[str(evaluate(expr))]
    if ctype in ("int", "unsigned", "unsigned char", "unsigned short", "long", "unsigned long", "short"):
        v = evaluate(expr)
        if pname == "pre":
            return "u64", f"0x{v & 0xFFFFFFFFFFFFFFFF:X}"
        if pname == "ownable":
            return "u32", f"0x{v & 0xFFFFFFFF:X}"
        if ctype == "unsigned short":
            return "u16", str(v)
        if ctype == "unsigned char":
            return "u8", str(v)
        return "i32", str(v)
    if ctype == "WeaponType":
        return "Option<Weapon>", enum_value("Weapon", "WeaponType", "WEAPON_", expr, True)
    if ctype == "WarheadType":
        return "Option<Warhead>", enum_value("Warhead", "WarheadType", "WARHEAD_", expr, True)
    if ctype == "ArmorType":
        return "Armor", enum_value("Armor", "ArmorType", "ARMOR_", expr, False)
    if ctype == "SpeedType":
        return "Option<Locomotion>", enum_value("Locomotion", "SpeedType", "SPEED_", expr, True)
    if ctype == "AnimType":
        return "Option<Anim>", enum_value("Anim", "AnimType", "ANIM_", expr, True)
    if ctype == "MissionType":
        return "Option<Mission>", enum_value("Mission", "MissionType", "MISSION_", expr, True)
    if ctype == "RTTIType":
        return "Rtti", enum_value("Rtti", "RTTIType", "RTTI_", expr, False)
    if ctype == "BSizeType":
        return "BuildingSize", enum_value("BuildingSize", "BSizeType", "BSIZE_", expr, False)
    if ctype == "LandType":
        return "Land", enum_value("Land", "LandType", "LAND_", expr, False)
    if ctype in ("char const *", "char const*"):
        name = re.sub(r"\(char const \*\)", "", expr).strip()
        if name in ("NULL", "0"):
            return "&'static [u8]", "&[]"
        return "&'static [u8]", "&[" + ", ".join(str(v) for v in lists[name]) + "]"
    if ctype == "DirType":
        return "Dir", f"Dir({evaluate(expr) & 0xFF})"
    if ctype == "MPHType":
        return "u8", str(evaluate(expr))
    if ctype == "COORDINATE":
        return "Coord", f"Coord::from_raw(0x{evaluate(expr) & 0xFFFFFFFF:08X})"
    if ctype in ("short const *", "short const*"):
        name = re.sub(r"\(short const \*\)", "", expr).strip()
        if name in ("NULL", "0"):
            return "&'static [i16]", "&[]"
        return "&'static [i16]", "&[" + ", ".join(str(v) for v in lists[name]) + "]"
    raise ValueError(f"unhandled C++ type {ctype!r} for {pname}")


def split_args(text):
    args, depth, cur, in_str = [], 0, [], False
    for ch in text:
        if ch == '"':
            in_str = not in_str
        if not in_str:
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
            elif ch == "," and depth == 0:
                args.append("".join(cur).strip())
                cur = []
                continue
        cur.append(ch)
    if "".join(cur).strip():
        args.append("".join(cur).strip())
    return args


def ctor_params(src, cls):
    m = re.search(cls + r"::" + cls + r"\s*\((.*?)\)\s*:", src, re.S)
    params = []
    for p in split_args(m.group(1)):
        p = " ".join(p.split())
        name = re.findall(r"(\w+)\s*$", p)[0]
        ctype = p[: -len(name)].strip()
        params.append((ctype, name))
    return params


TYPE_H = strip_comments(read("TYPE.H"))


def ctor_defaults(cls):
    """Default arguments from the constructor's declaration in TYPE.H."""
    m = re.search(r"\b" + cls + r"\s*\(([^;{]*?)\)\s*;", TYPE_H, re.S)
    out = {}
    for p in split_args(m.group(1)) if m else []:
        if "=" in p:
            decl, value = p.split("=", 1)
            out[decl.split()[-1]] = value.strip()
    return out


def static_lists(src):
    lists = {}
    for name, body in re.findall(r"static\s+short\s+const\s+(\w+)\s*\[\]\s*=\s*\{(.*?)\}\s*;", src, re.S):
        values = [evaluate(v) for v in split_args(body)]
        lists[name] = [v for v in values if v != 32767]
    # Icon lists (`AltIcons`) end at -1.
    for name, body in re.findall(r"static\s+char\s+const\s+(\w+)\s*\[\]\s*=\s*\{(.*?)\}\s*;", src, re.S):
        values = [evaluate(v) for v in split_args(body)]
        lists[name] = values[: values.index(-1)] if -1 in values else values
    return lists


SKIP = {"name", "ininame", "do_table", "type", "airtype", "iconset", "fullname", "terrain"}
DOCS = {"level": "Build level (tech level).", "pre": "Prerequisite buildings, as a bit mask of `BuildingType` ids.",
        "ownable": "Houses that may own it, as a bit mask of `House` ids.", "strength": "Full strength as written (buildings get double this; see `max_strength`).",
        "sightrange": "Sight range in cells.", "cost": "Cost to build, in credits.", "scenario": "First scenario it's available in.",
        "risk": "Risk rating.", "reward": "Reward rating.", "ammo": "Shots before reloading (-1 unlimited).",
        "maxSpeed": "Top speed on the game's 0..=255 scale.", "maxspeed": "Top speed on the game's 0..=255 scale.",
        "rot": "Rate of turn.", "speed": "How it moves.", "exp": "Explosion when destroyed.", "order": "Default mission.", "deforder": "Default mission.",
        "toffset": "Turret offset along the body centreline.", "exitpoint": "Exit point for produced units.",
        "power": "Power produced.", "is_unsellable": "Can't be sold.", "drain": "Power consumed.", "capacity": "Tiberium storage.", "size": "Footprint.",
        "exitlist": "Preferred exit cells, as offsets.", "sizelist": "Occupied cells, as offsets from the top-left.",
        "overlap": "Overlapped (drawn over) cells, as offsets.", "occupy": "Occupied cells, as offsets from its cell.",
        "centerbase": "Offset of its base from its cell's corner, in leptons.", "toBuild": "What it produces.", "dir": "Starting idle facing.",
        "arming": "Ticks before it can explode.", "theater": "Theaters it exists in, as a bit mask of `Theater` ids.",
        "land": "The ground it makes.", "altland": "The ground its `alt_icons` make instead.",
        "alticons": "Icons that make `alt_land` instead of `land`.", "width": "Width in cells.", "height": "Height in cells.",
        "ground": "The ground it makes (anything but clear overrides the template).",
        "damagelevels": "Damage stages, for walls.", "damagepoints": "Damage per stage, for walls.", "range": "Range override.", "warhead": "Warhead.", "explosion": "Explosion on impact."}


def snake(n):
    s = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", n).lower()
    return {"type": "kind", "maxspeed": "max_speed", "sightrange": "sight_range", "toffset": "turret_offset",
            "deforder": "order", "exp": "explosion", "pre": "prerequisites", "ownable": "owners", "level": "tech_level",
            "sizelist": "occupy", "exitlist": "exits", "to_build": "builds", "firelaunch": "fire_launch",
            "pronelaunch": "prone_launch", "altland": "alt_land", "alticons": "alt_icons", "damagelevels": "damage_levels",
            "damagepoints": "damage_points", "isradarvisible": "is_radar_visible", "iswooden": "is_wooden", "istarget": "is_target",
            "iscrushable": "is_crushable", "istiberium": "is_tiberium", "walltype": "is_wall", "iscrate": "is_crate",
            "high": "is_high", "theater": "theaters", "centerbase": "center_base", "tobuild": "builds", "speed": "locomotion", "max_speed": "max_speed", "sframe": "start_facing", "exitpoint": "exit_point", "canenter": "can_enter"}.get(s, s)


objects_rs = [
    "//! Tiberian Dawn's built-in object data: the constructor arguments of every type in the",
    "//! `*DATA.CPP` files, and the weapon and warhead tables in `CONST.CPP`.",
    "//!",
    "//! @generated by `rust/tools/gen_td_data.py`. Do not edit by hand.",
    "",
    "#![allow(clippy::unreadable_literal, clippy::struct_excessive_bools, clippy::too_many_lines)]",
    "",
    "use cnc_core::geometry::{Coord, Dir};",
    "",
    "use super::enums::{Anim, Armor, Bullet, BuildingSize, Land, Locomotion, Rtti, Voc, Warhead, Weapon};",
    "use crate::scenario::Mission;",
    "use crate::{AircraftType, BuildingType, InfantryType, OverlayType, TemplateType, TerrainType, UnitType};",
    "",
]

GROUPS = [
    ("UnitData", "UDATA.CPP", "UnitTypeClass", "UnitType", "UNIT_", "UnitType", "UNITS", "A vehicle type's data."),
    ("InfantryData", "IDATA.CPP", "InfantryTypeClass", "InfantryType", "INFANTRY_", "InfantryType", "INFANTRY", "An infantry type's data."),
    ("BuildingData", "BDATA.CPP", "BuildingTypeClass", "BuildingType", "STRUCT_", "StructType", "BUILDINGS", "A building type's data."),
    ("AircraftData", "AADATA.CPP", "AircraftTypeClass", "AircraftType", "AIRCRAFT_", "AircraftType", "AIRCRAFT", "An aircraft type's data."),
    ("BulletData", "BBDATA.CPP", "BulletTypeClass", "Bullet", "BULLET_", "BulletType", "BULLETS", "A projectile type's data."),
    ("TemplateData", "CDATA.CPP", "TemplateTypeClass", "TemplateType", "TEMPLATE_", "TemplateType", "TEMPLATES", "A terrain template's data."),
    ("TerrainData", "TDATA.CPP", "TerrainTypeClass", "TerrainType", "TERRAIN_", "TerrainType", "TERRAINS", "A terrain object's (tree's, rock's) data."),
    ("OverlayData", "ODATA.CPP", "OverlayTypeClass", "OverlayType", "OVERLAY_", "OverlayType", "OVERLAYS", "An overlay type's data."),
]

counts = {}
for struct, file, cls, rust_enum, prefix, cname, table, doc in GROUPS:
    src = preprocess(read(file), set(defined))
    # Plain numeric #defines local to the file (e.g. TDATA.CPP's TREE_NORMAL).
    for name, expr in re.findall(r"^\s*#\s*define\s+(\w+)\s+([^\n]+)$", src, re.M):
        if name not in CONSTS:
            try:
                CONSTS[name] = evaluate(expr)
            except (ValueError, SyntaxError, TypeError, NameError):
                pass
    params = ctor_params(src, cls)
    lists = static_lists(src)
    by_var = {}
    for var, body in re.findall(cls + r"\s+const\s+(\w+)\s*\((.*?)\)\s*;", src, re.S):
        args = split_args(body)
        defaults = ctor_defaults(cls)
        args += [defaults[p[1]] for p in params[len(args):] if p[1] in defaults]
        if len(args) != len(params):
            sys.exit(f"{file} {var}: {len(args)} arguments for {len(params)} parameters")
        by_var[var] = dict(zip([p[1] for p in params], args))
    # The game looks types up through `Pointers[]`, so that order is the id order (it can
    # disagree with the constructor's own type argument: the sniper bullet says BULLET_BULLET).
    pointers = re.search(cls + r"::Pointers\[\w+\]\s*=\s*\{(.*?)\}\s*;", src, re.S).group(1)
    order = re.findall(r"&\s*(\w+)", pointers)
    if len(order) != len(real_variants(cname, prefix)):
        sys.exit(f"{file}: {len(order)} pointers for {len(real_variants(cname, prefix))} {cname} values")
    fields = [(ct, pn) for ct, pn in params if pn not in SKIP]
    types = {}
    rows = []
    for var in order:
        entry = by_var[var]
        kind_ident = entry[params[0][1]].strip()
        if rust_enum == "Bullet":
            kind = enum_value("Bullet", "BulletType", "BULLET_", kind_ident, False)
        else:
            kind = existing_variant(rust_enum, cname, prefix, kind_ident)
        values = []
        for ct, pn in fields:
            t, v = convert(ct, pn, entry[pn], lists)
            types[pn] = t
            values.append((pn, v))
        rows.append((kind, entry.get("ininame", '""').strip(), values))
    counts[struct] = len(rows)
    objects_rs += [f"/// {doc}", "#[derive(Debug, Clone, Copy, PartialEq, Eq)]", f"pub struct {struct} {{",
                   "    /// The constructor's type argument (normally the slot's own type).", f"    pub kind: {rust_enum},", "    /// INI / art name.", "    pub ini_name: &'static str,"]
    for ct, pn in fields:
        objects_rs += [f"    /// {DOCS.get(pn, f'`{pn}` constructor argument.')}", f"    pub {snake(pn)}: {types[pn]},"]
    objects_rs += ["}", "", f"/// Every {struct[:-4].lower()} type defined in `{file}`, indexed by `{rust_enum}` id.",
                   f"pub static {table}: [{struct}; {len(rows)}] = ["]
    for kind, ini, values in rows:
        objects_rs.append(f"    {struct} {{")
        objects_rs.append(f"        kind: {kind},")
        objects_rs.append(f"        ini_name: {ini},")
        for pn, v in values:
            objects_rs.append(f"        {snake(pn)}: {v},")
        objects_rs.append("    },")
    objects_rs += ["];", ""]

# Weapons and warheads (CONST.CPP).
const_src = preprocess(read("CONST.CPP"), set(defined))
weapons = re.search(r"WeaponTypeClass const Weapons\[WEAPON_COUNT\]\s*=\s*\{(.*?)\};", const_src, re.S).group(1)
weapon_rows = [split_args(r) for r in re.findall(r"\{([^{}]*)\}", weapons)]
warheads = re.search(r"WarheadTypeClass const Warheads\[WARHEAD_COUNT\]\s*=\s*\{(.*?)\n\};", const_src, re.S).group(1)
warhead_rows = re.findall(r"\{\s*([^{},]+),([^{},]+),([^{},]+),([^{},]+),\s*\{([^{}]*)\}\s*\}", warheads)
weapon_variants = real_variants("WeaponType", "WEAPON_")
warhead_variants = real_variants("WarheadType", "WARHEAD_")
if len(weapon_rows) != len(weapon_variants) or len(warhead_rows) != len(warhead_variants):
    sys.exit(f"table sizes: {len(weapon_rows)} weapons for {len(weapon_variants)}, {len(warhead_rows)} warheads for {len(warhead_variants)}")
objects_rs += ["/// A weapon's data (`WeaponTypeClass`).", "#[derive(Debug, Clone, Copy, PartialEq, Eq)]", "pub struct WeaponData {",
               "    /// Which weapon this is.", "    pub kind: Weapon,", "    /// Projectile fired.", "    pub fires: Option<Bullet>,",
               "    /// Damage per shot.", "    pub attack: u8,", "    /// Rate of fire (ticks between shots).", "    pub rof: u8,",
               "    /// Range in leptons.", "    pub range: i32,", "    /// Sound when fired.", "    pub sound: Option<Voc>,",
               "    /// Muzzle animation.", "    pub anim: Option<Anim>,", "}", "",
               "/// Every weapon, in `Weapon` id order.", f"pub static WEAPONS: [WeaponData; {len(weapon_rows)}] = ["]
for (ident, _), row in zip(weapon_variants, weapon_rows):
    bullet, attack, rof, rng, sound, anim = row
    objects_rs += ["    WeaponData {", f"        kind: Weapon::{rust_ident(ident, 'WEAPON_')},",
                   f"        fires: {enum_value('Bullet', 'BulletType', 'BULLET_', bullet, True)},",
                   f"        attack: {evaluate(attack)},", f"        rof: {evaluate(rof)},", f"        range: {evaluate(rng)},",
                   f"        sound: {enum_value('Voc', 'VocType', 'VOC_', sound, True)},",
                   f"        anim: {enum_value('Anim', 'AnimType', 'ANIM_', anim, True)},", "    },"]
objects_rs += ["];", "", "/// A warhead's data (`WarheadTypeClass`).", "#[derive(Debug, Clone, Copy, PartialEq, Eq)]", "pub struct WarheadData {",
               "    /// Which warhead this is.", "    pub kind: Warhead,", "    /// Damage spread factor.", "    pub spread_factor: i32,",
               "    /// Destroys walls.", "    pub wall_destroyer: bool,", "    /// Destroys trees.", "    pub wood_destroyer: bool,",
               "    /// Destroys Tiberium.", "    pub tiberium_destroyer: bool,",
               "    /// Damage against each `Armor`, in 1/256ths.", "    pub modifier: [u16; 5],", "}", "",
               "/// Every warhead, in `Warhead` id order.", f"pub static WARHEADS: [WarheadData; {len(warhead_rows)}] = ["]
for (ident, _), row in zip(warhead_variants, warhead_rows):
    spread, wall, wood, tib, mods = row
    m = [evaluate(v) for v in split_args(mods)]
    b = lambda e: "true" if evaluate(e) else "false"
    objects_rs += ["    WarheadData {", f"        kind: Warhead::{rust_ident(ident, 'WARHEAD_')},", f"        spread_factor: {evaluate(spread)},",
                   f"        wall_destroyer: {b(wall)},", f"        wood_destroyer: {b(wood)},", f"        tiberium_destroyer: {b(tib)},",
                   f"        modifier: [{', '.join(str(v) for v in m)}],", "    },"]
objects_rs += ["];", ""]

raw_const = strip_comments(read("CONST.CPP"))
for name, value in re.findall(r"#define\s+(S\d)\s+(0x[0-9A-Fa-f]+)", raw_const):
    CONSTS[name] = int(value, 16)
ground = re.search(r"GroundType const Ground\[LAND_COUNT\]\s*=\s*\{(.*?)\n\};", const_src, re.S).group(1)
ground_rows = re.findall(r"\{\s*[^{},]+,\s*\{([^{}]*)\}\s*,\s*(\w+)\s*\}", ground)
land_variants = real_variants("LandType", "LAND_")
if len(ground_rows) != len(land_variants):
    sys.exit(f"{len(ground_rows)} ground rows for {len(land_variants)} land types")
objects_rs += ["/// How a kind of ground affects movement and building (`GroundType`).", "#[derive(Debug, Clone, Copy, PartialEq, Eq)]",
               "pub struct GroundData {", "    /// Which ground this is.", "    pub land: Land,",
               "    /// Speed on it for each `Locomotion`, 0 (impassable) to 255 (full speed).", "    pub speed: [u8; 7],",
               "    /// Whether buildings can be placed on it.", "    pub buildable: bool,", "}", "",
               "/// Every kind of ground, in `Land` id order (`Ground[]`).", f"pub static GROUND: [GroundData; {len(ground_rows)}] = ["]
for (ident, _), (costs, build) in zip(land_variants, ground_rows):
    c = [evaluate(v) for v in split_args(costs)]
    objects_rs += ["    GroundData {", f"        land: Land::{rust_ident(ident, 'LAND_')},", f"        speed: [{', '.join(str(v) for v in c)}],",
                   f"        buildable: {'true' if evaluate(build) else 'false'},", "    },"]
objects_rs += ["];", ""]

OUT.mkdir(parents=True, exist_ok=True)
(OUT / "enums.rs").write_text("\n".join(enums_rs))
(OUT / "objects.rs").write_text("\n".join(objects_rs))
print("wrote", ", ".join(f"{v} {k}" for k, v in counts.items()), f"{len(weapon_rows)} weapons, {len(warhead_rows)} warheads")
