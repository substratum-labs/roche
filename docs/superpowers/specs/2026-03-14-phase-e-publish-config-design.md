# Phase E4: npm/PyPI Publish Config — Design Spec

## Goal

Set up release workflows for `roche-sandbox` on npm and PyPI, triggered by git tags.

## Architecture

```
.github/workflows/
├── ci.yml                    # existing — tests
├── publish-npm.yml           # new — npm release
└── publish-pypi.yml          # new — PyPI release
```

## Trigger

Both workflows trigger on version tags:
```yaml
on:
  push:
    tags: ["v*"]
```

## npm Workflow

```yaml
publish-npm:
  name: Publish to npm
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
      with:
        node-version: "20"
        registry-url: "https://registry.npmjs.org"
    - uses: arduino/setup-protoc@v3
      with:
        version: "28.x"
    - run: cd sdk/typescript && npm ci
    - run: cd sdk/typescript && bash scripts/proto-gen.sh
    - run: cd sdk/typescript && npm run build
    - run: cd sdk/typescript && npm test
    - run: cd sdk/typescript && npm publish
      env:
        NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

## PyPI Workflow

```yaml
publish-pypi:
  name: Publish to PyPI
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-python@v5
      with:
        python-version: "3.12"
    - uses: arduino/setup-protoc@v3
      with:
        version: "28.x"
    - run: pip install build twine
    - run: cd sdk/python && bash scripts/proto-gen.sh
    - run: cd sdk/python && python -m build
    - run: cd sdk/python && pip install -e ".[dev]"
    - run: pytest sdk/python/tests/ -v
    - run: cd sdk/python && twine upload dist/*
      env:
        TWINE_USERNAME: __token__
        TWINE_PASSWORD: ${{ secrets.PYPI_TOKEN }}
```

## Package Config Verification

### npm (`sdk/typescript/package.json`)
- Verify `name: "roche-sandbox"`
- Verify `main`, `types` fields point to build output
- Add `files` field to include only dist + generated
- Add `build` script (tsc)
- Add `prepublishOnly` script (build + test)

### PyPI (`sdk/python/pyproject.toml`)
- Verify `name = "roche-sandbox"`
- Verify `[build-system]` uses setuptools or hatchling
- Add classifiers, description, URLs
- Ensure `package-data` includes generated proto files

## Secrets Required

| Secret | Purpose |
|--------|---------|
| `NPM_TOKEN` | npm publish authentication |
| `PYPI_TOKEN` | PyPI publish authentication |

## Non-Goals

- Automated version bumping (manual for now)
- Changelog generation
- Cargo publish for roche-core/roche-cli (separate workflow later)
