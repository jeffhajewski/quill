# GitHub Actions Workflows

This directory contains CI/CD workflows for the Quill project.

## Workflows

### CI (`ci.yml`)

Runs on every push and pull request to main:
- **Test Suite**: Runs tests on Ubuntu and macOS with stable and beta Rust
- **Rustfmt**: Checks code formatting
- **Clippy**: Runs linter checks
- **Build**: Builds all crates in release mode
- **Docs**: Validates documentation builds

### Documentation (`docs.yml`)

Builds and deploys documentation to GitHub Pages:
- Builds API documentation with `cargo doc`
- Creates a documentation index page
- Copies markdown guides
- Deploys to GitHub Pages

**Triggers:**
- Push to main (when docs-related files change)
- Manual workflow dispatch

## Setting Up GitHub Pages

To enable GitHub Pages for this repository:

1. **Go to Repository Settings**
   - Navigate to your repository on GitHub
   - Click on "Settings"

2. **Configure Pages**
   - In the left sidebar, click "Pages"
   - Under "Source", select "GitHub Actions"
   - Click "Save"

3. **Trigger Deployment**
   - Push a commit to main, or
   - Go to Actions → Documentation → Run workflow

4. **Access Documentation**
   - Your docs will be available at: `https://<username>.github.io/<repo>/`
   - Direct link to API docs: `https://<username>.github.io/<repo>/quill_core/`

## Workflow Permissions

The documentation workflow requires the following permissions:

```yaml
permissions:
  contents: read
  pages: write
  id-token: write
```

These are set automatically in the workflow file.

## Caching

Both workflows use GitHub Actions cache to speed up builds:
- Cargo registry
- Cargo index
- Build artifacts

This significantly reduces build times for subsequent runs.

## Troubleshooting

### Documentation Build Fails

If the documentation build fails:
1. Check that all crates compile successfully locally: `cargo doc --no-deps --workspace`
2. Verify markdown files are valid
3. Check workflow logs in the Actions tab

### Pages Not Deploying

If pages don't deploy:
1. Verify GitHub Pages is enabled in repository settings
2. Check that the workflow has the correct permissions
3. Ensure the `deploy` job completed successfully
4. Wait a few minutes for DNS propagation

### Permission Errors

If you see permission errors:
1. Go to Settings → Actions → General
2. Under "Workflow permissions", select "Read and write permissions"
3. Check "Allow GitHub Actions to create and approve pull requests"
4. Save changes

## Local Testing

To test documentation builds locally:

```bash
# Build API docs
cargo doc --no-deps --workspace --lib

# Open in browser
open target/doc/index.html

# Build with all dependencies
cargo doc --workspace --lib
```

## Customization

### Changing the Documentation Theme

The workflow creates a custom index page. To modify it:
- Edit the HTML in `.github/workflows/docs.yml` under "Create documentation index"

### Adding More Documentation

To add additional documentation:
1. Add markdown files to the `docs/` directory
2. The workflow automatically copies them to `target/doc/guide/`
3. Update the index page to link to new docs

### Configuring rustdoc

To customize rustdoc output:
- Add `#![doc = include_str!("../README.md")]` to lib.rs files
- Use doc attributes: `#![doc(html_logo_url = "...")]`
- Set RUSTDOCFLAGS in the workflow

## Status Badges

Add these badges to your README.md:

```markdown
![CI](https://github.com/<username>/<repo>/workflows/CI/badge.svg)
![Documentation](https://github.com/<username>/<repo>/workflows/Documentation/badge.svg)
```

## Manual Deployment

To manually trigger documentation deployment:
1. Go to Actions tab
2. Select "Documentation" workflow
3. Click "Run workflow"
4. Select the main branch
5. Click "Run workflow"
