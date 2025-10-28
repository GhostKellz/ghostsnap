# 👋 Welcome to Ghostsnap!

## What is Ghostsnap?

Ghostsnap is a **fast, secure, and modern backup tool** built in Rust. Think of it as "restic for the cloud-native era" with first-class support for MinIO, Azure, AWS S3, and HestiaCP integration.

**Current Status**: Alpha (v0.1.0) → Working towards RC1 (v0.9.0)

---

## 🚀 New Here? Start With These Files

### 1. Understand the Project
📖 **[README.md](README.md)** - Project overview, features, installation  
📋 **[SPEC.md](SPEC.md)** - Technical specification and architecture  
🎯 **[RC1_SUMMARY.md](RC1_SUMMARY.md)** - Current state and RC1 goals

### 2. Get Started Developing
⚡ **[QUICK_START_RC1.md](QUICK_START_RC1.md)** - Week 1-2 quick wins to get started  
📝 **[TODO.md](TODO.md)** - Complete task list (36 weeks to RC1)  
🎯 **[PRIORITY_MATRIX.md](PRIORITY_MATRIX.md)** - What to work on first

### 3. Understand the Codebase
```
ghostsnap/
├── cli/           # Command-line interface
├── core/          # Core backup logic (chunking, crypto, repository)
├── backends/      # Storage backends (S3, Azure, MinIO, Local)
├── integrations/  # HestiaCP and other integrations
└── target/        # Build artifacts (ignored in git)
```

---

## 🎯 What's the Goal?

Get Ghostsnap to **Release Candidate 1 (RC1)** - a production-ready backup solution that:
- ✅ Backs up and restores data reliably
- ✅ Works with all major cloud storage providers
- ✅ Integrates seamlessly with HestiaCP
- ✅ Has comprehensive tests and documentation
- ✅ Is ready for beta testing in production

---

## 🔥 Quick Start Options

### Option 1: I Want to Contribute Code
**Time**: 1-2 hours to get productive

1. **Set up your environment**
   ```bash
   git clone https://github.com/ghostkellz/ghostsnap
   cd ghostsnap
   cargo build
   cargo test
   ```

2. **Pick a task from Week 1-2 Quick Wins**
   - Open [QUICK_START_RC1.md](QUICK_START_RC1.md)
   - Pick a task (start with "Fix deprecation warnings")
   - Create a branch: `git checkout -b fix/deprecations`

3. **Read the relevant code**
   - Check [SPEC.md](SPEC.md) for architecture
   - Browse the module you're working on
   - Run `cargo doc --open` for API docs

4. **Make your changes**
   - Write tests first (TDD)
   - Run `cargo clippy` frequently
   - Document public APIs

5. **Submit a PR**
   - Push your branch
   - Create PR with clear description
   - Respond to review feedback

### Option 2: I Want to Review and Plan
**Time**: 30 minutes

1. **Review current state**
   - Read [RC1_SUMMARY.md](RC1_SUMMARY.md)
   - Check build status: `cargo build`
   - Run tests: `cargo test`

2. **Review the plan**
   - Read [TODO.md](TODO.md) for complete roadmap
   - Check [PRIORITY_MATRIX.md](PRIORITY_MATRIX.md) for what's most important

3. **Provide feedback**
   - Open a GitHub Discussion
   - Comment on existing issues
   - Suggest improvements

### Option 3: I Want to Use It (Brave!)
**Time**: 30 minutes  
**Warning**: Alpha software! Expect bugs!

1. **Build the binary**
   ```bash
   cargo build --release
   ./target/release/ghostsnap --version
   ```

2. **Initialize a repository**
   ```bash
   # Local repository
   ./target/release/ghostsnap init /backup/repo

   # MinIO repository (if you have MinIO running)
   ./target/release/ghostsnap init \
     --backend minio \
     --endpoint http://localhost:9000 \
     --bucket backups \
     --access-key minioadmin \
     --secret-key minioadmin
   ```

3. **Try a backup** (may not work completely yet!)
   ```bash
   ./target/release/ghostsnap backup /path/to/data \
     --repo /backup/repo
   ```

4. **Report bugs**
   - Open GitHub issue with details
   - Include logs and error messages
   - Help us improve!

---

## 📊 Project Status Dashboard

### Core Features
| Feature | Status | Notes |
|---------|--------|-------|
| Chunking | ✅ Working | FastCDC implemented |
| Encryption | ✅ Working | ChaCha20-Poly1305 |
| Repository | 🚧 Partial | Init/open works, needs pack management |
| Backup | 🚧 Partial | Scaffolded, needs completion |
| Restore | 🚧 Partial | Scaffolded, needs completion |
| Deduplication | 🚧 Partial | Chunk-level dedup works |

### Backends
| Backend | Status | Notes |
|---------|--------|-------|
| Local | ✅ Working | Full implementation |
| S3 | ⚠️ Needs Fix | Deprecation warnings |
| MinIO | ⚠️ Needs Fix | Deprecation warnings |
| Azure | 🚧 Partial | Scaffolded, needs completion |
| Backblaze B2 | ❌ Missing | Not started |

### Testing & Quality
| Area | Status | Notes |
|------|--------|-------|
| Unit Tests | ⚠️ Minimal | 2 tests only |
| Integration Tests | ❌ Missing | Need end-to-end tests |
| Documentation | 🚧 Partial | README exists, needs more |
| CI/CD | ❌ Missing | No automation yet |

**Legend**: ✅ Done | 🚧 In Progress | ⚠️ Needs Attention | ❌ Not Started

---

## 🎯 Next Steps (Leadership)

### If You're the Project Lead
1. ✅ Review [TODO.md](TODO.md) and [PRIORITY_MATRIX.md](PRIORITY_MATRIX.md)
2. ✅ Approve or adjust the roadmap
3. ⏭️ Set up GitHub Projects with tasks (import [RC1_TASKS.csv](RC1_TASKS.csv))
4. ⏭️ Recruit developers (1-3 people recommended)
5. ⏭️ Define sprint schedule (2-week sprints suggested)
6. ⏭️ Set up communication channels (Discord, Slack, etc.)
7. ⏭️ Start Week 1 tasks!

### If You're a Core Developer
1. ✅ Read [SPEC.md](SPEC.md) to understand architecture
2. ✅ Review [QUICK_START_RC1.md](QUICK_START_RC1.md)
3. ⏭️ Pick a Week 1 task (deprecation fixes are easiest)
4. ⏭️ Set up local development environment
5. ⏭️ Submit your first PR
6. ⏭️ Join daily standups (if established)

### If You're a Contributor
1. ✅ Read [README.md](README.md) and [SPEC.md](SPEC.md)
2. ⏭️ Check GitHub Issues for "good first issue" labels
3. ⏭️ Join GitHub Discussions to introduce yourself
4. ⏭️ Pick a small task to get started
5. ⏭️ Ask questions - we're here to help!

---

## 💡 Key Insights from Current State

### What's Good
- ✅ **Solid foundation**: Core crypto and chunking work well
- ✅ **Clean architecture**: Well-organized workspace structure
- ✅ **Modern stack**: Rust 2024 edition, tokio async
- ✅ **Clear vision**: Good spec and roadmap

### What Needs Work
- ⚠️ **Incomplete features**: Backup/restore need finishing
- ⚠️ **Minimal testing**: Only 2 unit tests exist
- ⚠️ **No automation**: No CI/CD pipeline
- ⚠️ **Limited docs**: Need comprehensive user docs

### Biggest Risks
1. **Data integrity** - Must ensure zero data loss
2. **Timeline slip** - 36 weeks is ambitious
3. **Resource constraints** - Needs dedicated developers
4. **Backend complexity** - Cloud APIs are tricky

---

## 📚 Essential Reading Order

**If you have 15 minutes**: Read this file + [RC1_SUMMARY.md](RC1_SUMMARY.md)

**If you have 1 hour**: Add [QUICK_START_RC1.md](QUICK_START_RC1.md) + [PRIORITY_MATRIX.md](PRIORITY_MATRIX.md)

**If you have 3 hours**: Add [TODO.md](TODO.md) + [SPEC.md](SPEC.md) + browse the code

**If you're committing**: Read everything above + run `cargo doc --open`

---

## 🔗 Important Links

### Documentation
- **[README.md](README.md)** - Start here
- **[SPEC.md](SPEC.md)** - Technical specification
- **[TODO.md](TODO.md)** - Complete task list (36 weeks)
- **[RC1_SUMMARY.md](RC1_SUMMARY.md)** - Executive summary
- **[QUICK_START_RC1.md](QUICK_START_RC1.md)** - Week 1-2 guide
- **[PRIORITY_MATRIX.md](PRIORITY_MATRIX.md)** - What to work on first
- **[BOLT_INTEGRATION.md](BOLT_INTEGRATION.md)** - Bolt integration plans

### Data Files
- **[RC1_TASKS.csv](RC1_TASKS.csv)** - Importable task list for GitHub Projects

### Project Management
- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: Questions and community
- **GitHub Projects**: Task board (to be set up)
- **GitHub Actions**: CI/CD (to be set up)

---

## ❓ FAQ

**Q: Is Ghostsnap production-ready?**  
A: No, it's currently in alpha. We're working towards RC1 (v0.9.0) which will be suitable for beta testing.

**Q: When will RC1 be ready?**  
A: Estimated 36 weeks with 1 developer, 18-24 weeks with a small team.

**Q: Can I use it now?**  
A: Only for testing/development. Do NOT use for critical data!

**Q: How can I help?**  
A: Review [QUICK_START_RC1.md](QUICK_START_RC1.md) and pick a task, or join discussions to provide feedback.

**Q: What's the tech stack?**  
A: Rust (2024 edition), Tokio (async), ChaCha20-Poly1305 (encryption), BLAKE3 (hashing), FastCDC (chunking).

**Q: Why another backup tool?**  
A: Ghostsnap focuses on modern cloud infrastructure (MinIO, Azure, S3) and HestiaCP integration, with a Rust-first approach.

**Q: What's the license?**  
A: MIT OR Apache-2.0 (dual-licensed, user's choice).

---

## 🎉 Let's Build This!

Ghostsnap has a clear vision, solid foundation, and comprehensive roadmap. We need:
- **Developers** to write code
- **Testers** to find bugs
- **Writers** to create docs
- **Users** to provide feedback
- **Advocates** to spread the word

**Every contribution matters!** Whether it's fixing a typo, writing a test, or implementing a feature - you're helping build a tool that will help thousands of people protect their data.

**Ready to contribute?** Pick a task from [QUICK_START_RC1.md](QUICK_START_RC1.md) and let's get started! 💪

---

## 📞 Get Help

- **Questions**: GitHub Discussions
- **Bugs**: GitHub Issues
- **Security**: security@ghostsnap.dev (if configured)
- **General**: hello@ghostsnap.dev (if configured)

---

**Welcome aboard! Let's build something great together! 🚀**

---

*Last Updated: 2025-10-02*  
*Document Version: 1.0*  
*Maintained by: Ghostsnap Core Team*
