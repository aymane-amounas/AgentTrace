# Pending CI workflow

`ci.yml` lives here instead of `.github/workflows/` because the initial push was
made with a token that lacked the GitHub `workflow` OAuth scope, so Git refused
to create files under `.github/workflows/`.

To activate CI (fmt, clippy, tests, viewer lint/build, Docker build, dependency
audit), move it into place and push with a `workflow`-scoped token:

```bash
git mv .github/workflows-pending/ci.yml .github/workflows/ci.yml
git commit -m "ci: enable GitHub Actions workflow"
git push
```

If the push is still rejected for scope, either refresh the token
(`gh auth refresh -h github.com -s workflow`) or add the file via the GitHub web
UI (which has the needed scope).
