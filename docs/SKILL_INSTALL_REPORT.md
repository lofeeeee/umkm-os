# Skill Installation Report

- Runtime skill root: `/home/ryasr/.dsh/skills` (DeepSeek Harness `user-dsh` layer).
- Installer: `/home/ryasr/.dsh/skills/skill-installer/scripts/install-skill-from-github.py`.
- Source: six GitHub repositories as supplied by the user, cloned at the default branch and pinned by commit in `docs/evidence/SKILL_SOURCE_MANIFEST.json`.
- Discovery rule: `SKILL.md` under a source repository's `tests/` directory is a test fixture and is excluded. The manifest lists every counted source directory, its pinned commit, `skill_name`, and installed destination; it has **97 rows / 97 unique skill names**, including `template-skill`.
- Installation result: 60 names from the manifest were absent and were installed; 37 names were already present from earlier versions and were left intact to avoid overwriting local skill customizations. All 97 manifest names have a directory at `/home/ryasr/.dsh/skills/<skill_name>/SKILL.md`.
- One source frontmatter defect was repaired after install: `software-factory` description is now quoted YAML.
- Duplicate cleanup: the installer-derived `template/` directory and `template-skill/` directory both exposed frontmatter name `template-skill`. The extra `template/` directory was removed; only `template-skill/` remains.
- Final runtime root: **125 directories** with `SKILL.md`, **125 unique frontmatter names**, and **0 invalid frontmatter files**. These 125 directories are the unit used below; `125 - 97 = 28` directories are outside the six supplied repositories. No claim mixes names with directories.
- Catalog refresh was observed in-session: newly installed skills became visible in `<available_skills>`; the refreshed catalog listed 126 entries before duplicate cleanup.

## Verification commands

```bash
# exact source/target PRD copy
sha256sum docs/PRD.md /home/ryasr/.dsh/attachments/v1/files/95/95d7d036dcf83e1d9e73c004cdbb153c159a874c29029186997e6d3cfb8266c4/prd.md
# 95d7d036... for both

# manifest: 97 rows / 97 unique names
python3 - <<'PY'
import json
m = json.load(open('docs/evidence/SKILL_SOURCE_MANIFEST.json'))
print(len(m['rows']), m['expected_unique_skill_names'])
PY

# installed directory count and frontmatter validity
python3 - <<'PY'
import glob, yaml, collections
files = glob.glob('/home/ryasr/.dsh/skills/*/SKILL.md')
errors = []
names = []
for path in files:
    text = open(path, encoding='utf-8', errors='replace').read()
    end = text.find('\n---', 4)
    if not text.startswith('---\n') or end < 0:
        errors.append(path); continue
    meta = yaml.safe_load(text[4:end])
    if not isinstance(meta, dict) or not meta.get('name') or not meta.get('description'):
        errors.append(path); continue
    names.append(meta['name'])
counts = collections.Counter(names)
print('dirs', len(files), 'unique names', len(counts), 'duplicates', {k:v for k,v in counts.items() if v>1}, 'errors', len(errors))
PY
# dirs 125 unique names 125 duplicates {} errors 0

# manifest coverage
python3 - <<'PY'
import json, os
m = json.load(open('docs/evidence/SKILL_SOURCE_MANIFEST.json'))
missing = [r['skill_name'] for r in m['rows'] if not os.path.isfile(f"/home/ryasr/.dsh/skills/{r['skill_name']}/SKILL.md")]
print('manifest rows', len(m['rows']), 'missing installed', len(missing), missing)
PY
# manifest rows 97 missing installed 0 []
```

## Pinned source commits

| Repository | Commit |
|---|---|
| ryasrk/engineering-skills | `e299afcead1542af72a8ec2b467f86485b115943` |
| pbakaus/impeccable | `c1e0e5492fbb524f5df84d635ecfae7dc438ec45` |
| nextlevelbuilder/ui-ux-pro-max-skill | `6ae86f61f520d6ca229d2f40e070410638c908c5` |
| heygen-com/hyperframes | `3ad7611f9fd36c1cef589c59b116ad7043c3b041` |
| mattpocock/skills | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` |
| anthropics/skills | `2c5958ecac7cee2b583967a2c4981d0f4a75c579` |
