# Launch checklist

This file lists the work to do before and during the public launch of kknow.
Do the phases in this order. Do not start a phase before the previous one is
complete: a launch gets one first impression.

State on 2026-10-30: version 0.7.1, MIT license, public repository, builds for
macOS (Apple Silicon), Linux and Windows, Homebrew and Scoop, a documentation
site, auto-update. No builds have a code signature. The README and the site
have no screenshots.

## Phase 1. The first start must work

A new user must open the app with no errors and no terminal commands.

- [ ] Join the Apple Developer Program ($99 each year).
- [ ] Sign and notarize the macOS app in `release.yml` (Tauri supports
      `APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
      `APPLE_PASSWORD`, `APPLE_TEAM_ID`).
- [ ] Remove the quarantine step from the Homebrew cask and the `xattr`
      instructions from the README and the site.
- [ ] Decide about Intel Macs: add an `x86_64-apple-darwin` (or universal)
      build, or say clearly on the site that there is no Intel build.
- [ ] Decide about Windows signing. Without it, SmartScreen shows a warning.
      At a minimum, tell users on the site what to click.
- [ ] Do a clean install on each platform on a machine (or a VM) that never
      had kknow: download, open, create a vault, write a note, search.
- [ ] Open a real Obsidian vault as it is. Check links, embeds, attachments
      and the `.obsidian` folder. Write down what does not work (see
      TODO.md, "Before the launch").
- [x] Check that the app shows a clear message, not a crash, for common
      problems: no access to a folder, a wrong S3 key, a git repository with
      no remote (see TODO.md, "Before the launch").

## Phase 2. The main feature for the launch

- [x] E2E (end-to-end) encryption for sync (see TODO.md, "Larger tasks").
      "Your notes are on your own S3, and only you can read them" is the
      strongest message for the self-hosted audience.
- [x] A page about encryption on the site: what is encrypted, where the key
      is, what happens if you lose the key.

## Phase 3. The shop window

A visitor decides in about 10 seconds. The top of the README and the site
must answer "what is it, and why not Obsidian".

- [ ] Write one sentence about the difference. A draft: "An open-source
      Markdown knowledge base: free sync through your own S3 or git, and a
      CLI for the same notes."
- [ ] Use this sentence in the GitHub description, the README, the site and
      all posts.
- [ ] Record a GIF or a video of 20–30 seconds: open a vault, write with live
      preview, search, sync. Put it at the top of the README and on the site
      home page.
- [ ] Add 3–4 screenshots (light and dark themes, search, settings) to the
      site.
- [ ] Add a comparison table to the site: kknow, Obsidian, Logseq, Joplin.
      Be honest: also show what kknow does not have (plugins, mobile apps,
      canvas).
- [ ] Add GitHub topics: `markdown`, `notes`, `knowledge-base`, `obsidian`,
      `rust`, `tauri`, `local-first`, `self-hosted`.
- [ ] Set the social preview image of the repository (1280×640).

## Phase 4. A place for feedback

- [ ] Issue templates: bug report (version, OS, steps) and feature request.
- [ ] Turn on GitHub Discussions for questions and ideas.
- [ ] `CONTRIBUTING.md`: how to build, how to run the tests, the rules from
      `CLAUDE.md` that apply to people too.
- [ ] `SECURITY.md`: where to report a vulnerability. This is important
      because the app stores S3 keys and syncs data.
- [ ] A short roadmap on the site from TODO.md. Users want to see that the
      project is alive.
- [ ] Decide how much time each week goes to issues. Write it down for
      yourself.

## Phase 5. Quiet start

Small communities first. Fix what they report before the big launch.

- [ ] Tell the truth about how kknow was made: an AI agent (Claude Code)
      wrote most of the code, and the author set the goals, made the design
      decisions and tested the app. Add a short "How this project is made"
      section to the README and the site. Use the same words in all posts.
- [ ] Before each post, read the rules of the community about AI-generated
      projects. Some communities do not allow them or ask for a label.
- [ ] r/rust: a post about the architecture (a core with no Tauri, tantivy,
      opendal, one core for the window and the CLI), with the AI disclosure.
- [ ] Tauri: the showcase and the `awesome-tauri` list.
- [ ] `awesome-rust` and "This Week in Rust".
- [ ] A Habr article in Russian: "how I made a real desktop app with an AI
      agent": the process, the rules in `CLAUDE.md` and `ARCHITECTURE.md`,
      what worked, what did not, how much time and money it took.
- [ ] alternativeto.net: add kknow as an alternative to Obsidian.
- [ ] Fix the problems from the feedback. Make a release.

## Phase 6. Main launch

- [ ] r/selfhosted: focus on S3 and git sync and on encryption.
- [ ] r/PKMS. In r/ObsidianMD, only answer questions; do not advertise.
- [ ] Show HN. Post on a weekday morning, US time. Stay online for the first
      4–6 hours and answer all comments.
- [ ] Product Hunt, if there is time for it (optional).
- [ ] After each launch: write down the stars, the downloads (GitHub release
      statistics), the issues and the top three complaints.

## After the launch

- [ ] Short posts about each important release.
- [ ] Goal: 100 people who use kknow each day. Only then think about money.
- [ ] GitHub Sponsors or Boosty: it costs nothing to turn on.
- [ ] A paid hosted sync with E2E encryption: consider it only with a
      user base. It is a business with servers, support and GDPR duties.
