#!/usr/bin/env python3
"""Generates the Rust object-type enums from the original C++ sources.

Enum variants and their numeric ids come from the `typedef enum` blocks in DEFINES.H; the
INI name of each variant comes from the first string literal in its `...TypeClass const`
definition in the matching *DATA.CPP file. Variants with no data definition are skipped
(they are sentinels or unused).

Usage (from the repository root):

    python3 rust/tools/gen_types.py

Rewrites rust/crates/cnc-td/src/types.rs and rust/crates/cnc-ra/src/types.rs.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# (rust name, C enum name, C enum prefix, data file, type class, doc)
COMMON = [
    ("UnitType", "UnitType", "UNIT_", "UDATA.CPP", "UnitTypeClass", "A ground vehicle type."),
    ("InfantryType", "InfantryType", "INFANTRY_", "IDATA.CPP", "InfantryTypeClass", "An infantry type."),
    ("BuildingType", "StructType", "STRUCT_", "BDATA.CPP", "BuildingTypeClass", "A structure type."),
    ("AircraftType", "AircraftType", "AIRCRAFT_", "AADATA.CPP", "AircraftTypeClass", "An aircraft type."),
    ("OverlayType", "OverlayType", "OVERLAY_", "ODATA.CPP", "OverlayTypeClass", "An overlay type (walls, resources, crates...)."),
    ("SmudgeType", "SmudgeType", "SMUDGE_", "SDATA.CPP", "SmudgeTypeClass", "A smudge type (craters, scorch marks)."),
    ("TerrainType", "TerrainType", "TERRAIN_", "TDATA.CPP", "TerrainTypeClass", "A terrain object type (trees, rocks)."),
    ("TemplateType", "TemplateType", "TEMPLATE_", "CDATA.CPP", "TemplateTypeClass", "A map tile template type."),
]
GAMES = {
    "TIBERIANDAWN": ("cnc-td", "Tiberian Dawn", COMMON),
    "REDALERT": (
        "cnc-ra",
        "Red Alert",
        COMMON + [("VesselType", "VesselType", "VESSEL_", "VDATA.CPP", "VesselTypeClass", "A naval vessel type.")],
    ),
}

# Symbols neither game defines: their blocks are dead code (e.g. `STRUCT_ROAD` in TD).
SKIP_IFDEFS = {"PETROGLYPH_EXAMPLE_MOD", "OBSOLETE", "NEVER", "ADVANCED"}


def strip_disabled(text: str) -> str:
    """Drops `#ifdef X ... #endif` blocks for the example-mod defines and `#if 0` blocks."""
    out, depth_skip, stack = [], 0, []
    for line in text.splitlines():
        s = line.strip()
        m = re.match(r"#\s*(ifdef|ifndef|if)\b\s*(.*)", s)
        if m:
            skip = (m.group(1) == "ifdef" and m.group(2).split()[0] in SKIP_IFDEFS) or (
                m.group(1) == "if" and m.group(2).replace("(", "").replace(")", "").strip() in ("0",)
            )
            stack.append(skip)
            depth_skip += skip
            continue
        if re.match(r"#\s*endif", s):
            if stack:
                depth_skip -= stack.pop()
            continue
        if re.match(r"#\s*else", s):
            if stack:
                prev = stack.pop()
                depth_skip -= prev
                # Only flip blocks we chose to skip; others keep their #else contents.
                stack.append(False)
            continue
        if depth_skip == 0:
            out.append(line)
    return "\n".join(out)


def parse_enum(defines: str, cname: str, prefix: str):
    m = re.search(r"typedef\s+enum\s+" + cname + r"\b[^{]*\{(.*?)\}\s*" + cname + r"\s*;", defines, re.S)
    if not m:
        sys.exit(f"enum {cname} not found")
    body = re.sub(r"/\*.*?\*/", "", m.group(1), flags=re.S)
    value, variants = -1, []
    for raw in body.split("\n"):
        comment = ""
        if "//" in raw:
            raw, comment = raw.split("//", 1)
        for part in raw.split(","):
            part = re.sub(r"/\*.*?\*/", "", part).strip()
            if not part:
                continue
            if "=" in part:
                ident, expr = (p.strip() for p in part.split("=", 1))
                try:
                    value = int(expr, 0)
                except ValueError:
                    # Aliases such as UNIT_FIRST=0 or X=Y don't introduce new variants.
                    continue
            else:
                ident = part
                value += 1
            if ident.endswith(("_COUNT", "_FIRST", "_NONE")) or value < 0:
                continue
            variants.append((ident, value, comment.strip()))
    return variants


def parse_names(data: str, tclass: str):
    names = {}
    for m in re.finditer(tclass + r"\s+const\s+\w+\s*\((.*?)\);", data, re.S):
        args = re.sub(r"//[^\n]*", "", m.group(1))
        ident = re.match(r"\s*(\w+)", args)
        lit = re.search(r'"([^"]*)"', args)
        if ident and lit:
            names.setdefault(ident.group(1), lit.group(1))
    return names


def rust_ident(ident: str, prefix: str) -> str:
    stem = ident[len(prefix):] if ident.startswith(prefix) else ident
    parts = [p for p in stem.split("_") if p]
    name = "".join(p[:1].upper() + p[1:].lower() for p in parts)
    if name[:1].isdigit():
        name = "T" + name
    return name


def generate(game_dir: str):
    crate, title, types = GAMES[game_dir]
    src = ROOT / game_dir
    defines = strip_disabled((src / "DEFINES.H").read_text(encoding="latin-1"))
    out = [
        "//! Object type enums for " + title + ".",
        "//!",
        "//! @generated by `rust/tools/gen_types.py` from `DEFINES.H` and the `*DATA.CPP` files.",
        "//! Do not edit by hand; re-run the generator instead.",
        "",
        "cnc_core::named_enum! {",
    ]
    blocks = []
    for rname, cname, prefix, datafile, tclass, doc in types:
        variants = parse_enum(defines, cname, prefix)
        data = strip_disabled((src / datafile).read_text(encoding="latin-1"))
        names = parse_names(data, tclass)
        repr_ty = "u8" if max(v for _, v, _ in variants) < 256 else "u16"
        lines = [f"    /// {doc}", f"    pub enum {rname}: {repr_ty} {{"]
        used = set()
        for ident, value, comment in variants:
            if ident not in names:
                continue
            vname = rust_ident(ident, prefix)
            if vname in used:
                vname += str(value)
            used.add(vname)
            comment = " ".join(comment.split())
            if comment:
                doc_line = f"{comment.rstrip('.')}. (`{ident}`)"
            else:
                doc_line = f"`{ident}` (INI name `{names[ident]}`)."
            lines.append(f"        /// {doc_line}")
            lines.append(f'        {vname} = {value} => "{names[ident]}",')
        lines.append("    }")
        blocks.append("\n".join(lines))
    out.append("\n}\n\ncnc_core::named_enum! {\n".join(blocks))
    out.append("}")
    path = ROOT / "rust" / "crates" / crate / "src" / "types.rs"
    path.write_text("\n".join(out) + "\n")
    print(f"wrote {path.relative_to(ROOT)}")


if __name__ == "__main__":
    for game in GAMES:
        generate(game)
