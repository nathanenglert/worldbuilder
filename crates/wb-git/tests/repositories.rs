//! Against real repositories on disk, built by the test.
//!
//! Nothing here is mocked. A version-control layer that has never been run against an
//! actual `.git` directory is a layer that works until the first time it matters, and the
//! four behaviours worth the most here — the nested refusal, the dirty-tree refusal, the
//! not-a-fast-forward refusal, and the scratch directory not counting as a change — are
//! all properties of a real repository rather than of this crate's own types.

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::atomic::{AtomicU32, Ordering};

use git2::{ConfigLevel, Repository};
use wb_git::{Error, Standing};

/// Point libgit2's config search at nothing.
///
/// Without this, every test would read whoever's `~/.gitconfig` is on the machine — and
/// `committing_without_a_configured_author_…` would pass or fail depending on whether the
/// developer has ever run `git config --global user.name`. Each repository sets its own
/// author locally instead, so what the tests exercise is the code and not the laptop.
fn hermetic_config() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        for level in
            [ConfigLevel::System, ConfigLevel::Global, ConfigLevel::XDG, ConfigLevel::ProgramData]
        {
            let _ = git2::opts::set_search_path(level, "/nonexistent/worldbuilder/test/config");
        }
    });
}

/// A directory that cleans up after itself, with a name unique to the run.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        hermetic_config();
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("wb-git-{name}-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("sandbox");
        Self { dir: dir.canonicalize().expect("canonical sandbox") }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, contents).expect("write");
}

/// The smallest thing that looks like a world folder.
fn seed_world(at: &Path) {
    write(&at.join("world.yaml"), "name: A Small World\ncalendar:\n  name: Plain\n");
    write(&at.join("entities/actors/one.md"), "---\nid: act_one\nname: One\ntype: person\n---\n");
}

/// A repository with an author configured locally and one commit in it.
fn repo_at(at: &Path) -> Repository {
    let repo = Repository::init(at).expect("init");
    configure_author(&repo);
    repo
}

/// Locally, because [`hermetic_config`] has taken the machine's own identity away — and
/// a repository this app started has no author until the writer's global config supplies
/// one, which in a test is nobody.
fn configure_author(repo: &Repository) {
    let mut config = repo.config().expect("config");
    config.set_str("user.name", "A Writer").expect("name");
    config.set_str("user.email", "writer@example.invalid").expect("email");
}

fn commit_all(standing: &Standing, message: &str) {
    wb_git::commit(standing, message).unwrap_or_else(|e| panic!("commit `{message}`: {e}"));
}

// ------------------------------------------------------------------- standing

#[test]
fn a_world_inside_a_bigger_repository_is_read_only_and_says_which_repository() {
    let sandbox = Sandbox::new("nested");
    let repo_root = sandbox.path();
    let world = repo_root.join("worlds/vashen");

    repo_at(repo_root);
    write(&repo_root.join("README.md"), "a repository that is about other things too\n");
    seed_world(&world);

    let standing = Standing::of(&world);
    assert_eq!(standing.slug(), "nested");
    assert_eq!(standing.prefix(), Path::new("worlds/vashen"));

    // Read-only half: available.
    let status = wb_git::status(&standing).expect("status of a nested world");
    assert!(status.unborn, "nothing committed yet");
    assert!(
        status.dirty.iter().all(|c| c.path.starts_with("worlds/vashen")),
        "only the world's own files, not the repository's: {:?}",
        status.dirty
    );

    // Mutating half: refused, and the message names the repository that would move.
    let refusal = wb_git::create_branch(&standing, "what-if/anything", false).unwrap_err();
    assert!(matches!(refusal, Error::NotRepoRoot { .. }));
    let said = refusal.to_string();
    assert!(said.contains(&repo_root.display().to_string()), "names the repository: {said}");
    assert!(said.contains("History and comparison still work"), "and what still works: {said}");
}

#[test]
fn a_world_folder_that_is_the_repository_gets_the_whole_surface() {
    let sandbox = Sandbox::new("root");
    seed_world(sandbox.path());
    repo_at(sandbox.path());

    let standing = Standing::of(sandbox.path());
    assert_eq!(standing.slug(), "root");
    assert_eq!(standing.prefix(), Path::new(""), "an empty prefix means all of it");
    assert!(standing.is_root());
}

#[test]
fn a_folder_nothing_tracks_has_no_standing_and_no_status() {
    let sandbox = Sandbox::new("untracked");
    seed_world(sandbox.path());

    let standing = Standing::of(sandbox.path());
    assert_eq!(standing.slug(), "none");
    assert!(matches!(wb_git::status(&standing), Err(Error::NotARepository { .. })));
}

/// The property the version chip rests on, now that it stops reporting a clean tree it
/// could not see.
///
/// `status` answers two different questions with one type — where the world stands, and
/// whether it could be looked at — and only the second one can be told apart by the
/// caller if this holds. A repository that answers "no branch, nothing unsaved" when it
/// is actually unreadable is the worst possible answer: it is the same shape as a saved,
/// healthy world.
#[test]
fn a_repository_that_cannot_be_read_is_an_error_rather_than_an_empty_status() {
    let sandbox = Sandbox::new("unreadable");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "everything saved");
    assert!(wb_git::status(&standing).expect("status").dirty.is_empty());

    // What an interrupted `git add` can leave behind, and the reason this is worth a test
    // rather than an assumption: it is damage the *writer's* own tools do, to a file this
    // app never writes.
    write(&sandbox.path().join(".git/index"), "not an index, and never was");

    assert!(
        wb_git::status(&standing).is_err(),
        "a repository that cannot be read must say so rather than report a clean tree"
    );
}

/// The branch name is not cosmetic: it is what canon resolves to.
///
/// `canon_of` looks for `origin/HEAD`, then `main`, then `master`. A world started on a
/// machine whose git still defaults to `master` would land on the third of those, so
/// every what-if the writer ever made would be measured against a branch chosen by which
/// version of git happened to be installed.
#[test]
fn a_world_that_has_just_been_started_is_a_repository_root_on_main() {
    let sandbox = Sandbox::new("init");
    seed_world(sandbox.path());
    assert_eq!(Standing::of(sandbox.path()).slug(), "none", "nothing tracks it yet");

    let standing = wb_git::init(sandbox.path()).expect("init");
    assert_eq!(standing.slug(), "root");

    let before = wb_git::status(&standing).expect("status");
    assert!(before.unborn, "a repository with nowhere to compare from yet");
    assert_eq!(before.branch.as_deref(), Some("main"), "on a branch before it has commits");
    assert_eq!(before.canon.as_deref(), Some("main"), "and it is its own canon");
    assert!(
        before.dirty.iter().any(|c| c.path == "world.yaml"),
        "and everything in the folder waiting to go into the first save point"
    );

    configure_author(&Repository::open(sandbox.path()).expect("open"));
    commit_all(&standing, "the world, such as it is");

    let after = wb_git::status(&standing).expect("status");
    assert_eq!(after.branch.as_deref(), Some("main"));
    assert_eq!(after.canon.as_deref(), Some("main"), "and it is its own canon");
}

/// A world folder inside a repository about other things gets its own history.
///
/// The alternative is honouring the outer repository, which is [`Standing::Nested`] —
/// the read-only arm, where making a save point is refused. A writer who asked for this
/// world to be tracked asked for the arm where that works.
#[test]
fn a_world_started_inside_a_bigger_repository_gets_a_history_of_its_own() {
    let sandbox = Sandbox::new("init-nested");
    let world = sandbox.path().join("worlds/somewhere");
    repo_at(sandbox.path());
    seed_world(&world);
    assert_eq!(Standing::of(&world).slug(), "nested");

    let standing = wb_git::init(&world).expect("init");
    assert_eq!(standing.slug(), "root");
    assert_eq!(standing.repo(), Some(world.canonicalize().expect("canonical").as_path()));
}

#[test]
fn starting_a_world_that_is_already_a_repository_leaves_its_history_alone() {
    let sandbox = Sandbox::new("init-again");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "the world as it stands");

    let again = wb_git::init(sandbox.path()).expect("init an existing repository");
    assert_eq!(again.slug(), "root");
    assert_eq!(
        wb_git::history(&again, 10).expect("history").commits.len(),
        1,
        "the save point that was already there"
    );
}

// ------------------------------------------------------------------- reading

#[test]
fn history_for_a_nested_world_shows_only_commits_that_touched_it() {
    let sandbox = Sandbox::new("subtree-history");
    let repo_root = sandbox.path();
    let world = repo_root.join("worlds/vashen");

    repo_at(repo_root);
    seed_world(&world);
    let whole = Standing::of(repo_root);
    commit_all(&whole, "the world arrives");

    write(&repo_root.join("notes-about-something-else.md"), "not the world\n");
    commit_all(&whole, "something else entirely");

    write(
        &world.join("entities/actors/two.md"),
        "---\nid: act_two\nname: Two\ntype: person\n---\n",
    );
    commit_all(&whole, "a second record");

    let nested = Standing::of(&world);
    let history = wb_git::history(&nested, 20).expect("history");
    let summaries: Vec<&str> = history.commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["a second record", "the world arrives"]);
    assert_eq!(history.scanned, 3, "it looked at all three to find the two");
    assert!(!history.truncated);

    // And from the repository root, all three, because there the world is everything.
    assert_eq!(wb_git::history(&whole, 20).expect("history").commits.len(), 3);
}

#[test]
fn materializing_a_nested_world_yields_the_world_folder_and_not_the_repository() {
    let sandbox = Sandbox::new("materialize");
    let repo_root = sandbox.path();
    let world = repo_root.join("worlds/vashen");

    repo_at(repo_root);
    write(&repo_root.join("unrelated.txt"), "should not travel\n");
    seed_world(&world);
    commit_all(&Standing::of(repo_root), "everything");

    let nested = Standing::of(&world);
    let into = sandbox.path().join("out");
    let scratch = wb_git::Scratch::at(&into).expect("scratch");
    wb_git::materialize(&nested, "HEAD", scratch.path()).expect("materialize");

    assert!(into.join("world.yaml").is_file(), "the world's own root file");
    assert!(into.join("entities/actors/one.md").is_file());
    assert!(!into.join("unrelated.txt").exists(), "and nothing from around it");
}

#[test]
fn materializing_an_older_revision_gives_back_what_it_said_then() {
    let sandbox = Sandbox::new("older");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "first");
    let first = wb_git::history(&standing, 1).expect("history").commits[0].full.clone();

    write(
        &sandbox.path().join("entities/actors/one.md"),
        "---\nid: act_one\nname: One Renamed\n---\n",
    );
    commit_all(&standing, "second");

    let into = std::env::temp_dir().join("wb-git-older-out");
    let scratch = wb_git::Scratch::at(&into).expect("scratch");
    wb_git::materialize(&standing, &first, scratch.path()).expect("materialize");

    let text = std::fs::read_to_string(scratch.path().join("entities/actors/one.md")).unwrap();
    assert!(text.contains("name: One"), "the old text");
    assert!(!text.contains("Renamed"), "and not the new one");
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_tree_is_skipped_rather_than_restored_pointing_nowhere() {
    let sandbox = Sandbox::new("symlink");
    seed_world(sandbox.path());
    std::os::unix::fs::symlink("world.yaml", sandbox.path().join("shortcut.yaml")).expect("link");
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "with a link in it");

    let into = sandbox.path().join("out");
    let scratch = wb_git::Scratch::at(&into).expect("scratch");
    wb_git::materialize(&standing, "HEAD", scratch.path()).expect("materialize");

    assert!(into.join("world.yaml").is_file());
    assert!(
        !into.join("shortcut.yaml").exists(),
        "a symlink restored into a scratch directory points somewhere unrelated to \
         where it pointed, which is worse than its absence"
    );
}

#[test]
fn a_revision_written_as_an_id_resolves_to_what_that_save_point_says_about_itself() {
    let sandbox = Sandbox::new("resolve");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "the siege, finally dated");
    let id = wb_git::history(&standing, 1).expect("history").commits[0].full.clone();

    // What the compare panel is handed when the writer clicks a row in the history: a
    // forty-character hash, and nothing a person would recognise their own work by.
    let by_id = wb_git::resolve(&standing, &id).expect("resolve an id");
    assert!(by_id.by_id, "written as an id, so there is no name to show");
    assert_eq!(by_id.commit.summary, "the siege, finally dated");

    // And what it is handed when they click a what-if: a name they chose, which must
    // come back unchanged rather than being replaced by its tip's message.
    wb_git::create_branch(&standing, "what-if/aldric-lived", false).expect("branch");
    let by_name = wb_git::resolve(&standing, "what-if/aldric-lived").expect("resolve a name");
    assert!(!by_name.by_id, "a name resolves to an id that is nothing like the string");
    assert_eq!(by_name.commit.full, id, "and it is still the same commit underneath");
}

#[test]
fn our_own_scratch_directory_never_makes_the_tree_look_dirty() {
    let sandbox = Sandbox::new("scratch-clean");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "clean");
    assert!(wb_git::status(&standing).expect("status").dirty.is_empty());

    // Deliberately no `.gitignore`: a writer's own world folder need not have one, and
    // that is the case where this could bite. One comparison must not make the next
    // branch switch impossible.
    let into = sandbox.path().join(".worldbuilder/compare/abc123");
    let scratch = wb_git::Scratch::at(&into).expect("scratch");
    wb_git::materialize(&standing, "HEAD", scratch.path()).expect("materialize");
    assert!(into.join("world.yaml").is_file(), "it really did write files");

    assert!(
        wb_git::status(&standing).expect("status").dirty.is_empty(),
        "the derived folder is not a change to the world"
    );
    wb_git::switch(&standing, "master")
        .or_else(|_| wb_git::switch(&standing, "main"))
        .expect("and switching is still possible with it there");
}

// ------------------------------------------------------- the four refusals

#[test]
fn committing_without_a_configured_author_says_which_two_commands_to_run() {
    let sandbox = Sandbox::new("no-author");
    seed_world(sandbox.path());
    Repository::init(sandbox.path()).expect("init"); // no user.name, no user.email

    let standing = Standing::of(sandbox.path());
    let refusal = wb_git::commit(&standing, "anything").unwrap_err();
    assert!(matches!(refusal, Error::NoAuthor));
    let said = refusal.to_string();
    assert!(said.contains("git config --global user.name"), "{said}");
    assert!(said.contains("git config --global user.email"), "{said}");
}

#[test]
fn switching_branches_with_unsaved_changes_is_refused_and_names_the_files() {
    let sandbox = Sandbox::new("dirty-switch");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "first");
    wb_git::create_branch(&standing, "what-if/aldric-lived", false).expect("branch");

    write(&sandbox.path().join("entities/actors/one.md"), "---\nid: act_one\nname: Changed\n---\n");

    let refusal = wb_git::switch(&standing, "what-if/aldric-lived").unwrap_err();
    let Error::Dirty { paths } = &refusal else { panic!("expected Dirty, got {refusal:?}") };
    assert_eq!(paths, &["entities/actors/one.md"]);
    assert!(refusal.to_string().contains("Make a save point first"), "and what to do about it");
}

#[test]
fn a_merge_that_would_not_fast_forward_is_refused_rather_than_attempted() {
    let sandbox = Sandbox::new("no-ff");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "the common ancestor");
    let canon = wb_git::status(&standing).expect("status").branch.expect("on a branch");

    wb_git::create_branch(&standing, "what-if", true).expect("branch and switch");
    write(&sandbox.path().join("entities/actors/two.md"), "---\nid: act_two\nname: Two\n---\n");
    commit_all(&standing, "on the what-if");

    // Canon moves too, so the histories have diverged.
    wb_git::switch(&standing, &canon).expect("back to canon");
    write(
        &sandbox.path().join("entities/actors/three.md"),
        "---\nid: act_three\nname: Three\n---\n",
    );
    commit_all(&standing, "on canon");
    wb_git::switch(&standing, "what-if").expect("back to the what-if");

    let refusal = wb_git::merge_into(&standing, &canon).unwrap_err();
    let Error::NotFastForward { behind, .. } = &refusal else { panic!("got {refusal:?}") };
    assert_eq!(*behind, 1);
    assert!(refusal.to_string().contains("job for a git client"), "and whose job it is");
}

#[test]
fn deleting_the_branch_you_are_standing_on_is_refused() {
    let sandbox = Sandbox::new("delete-current");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "first");
    wb_git::create_branch(&standing, "what-if", true).expect("branch");

    let refusal = wb_git::delete_branch(&standing, "what-if").unwrap_err();
    assert!(matches!(refusal, Error::CurrentBranch { .. }));
    assert!(refusal.to_string().contains("switch somewhere else first"));
}

#[test]
fn saving_when_nothing_changed_is_refused_rather_than_recorded_as_an_empty_point() {
    let sandbox = Sandbox::new("nothing-to-save");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "first");

    assert!(matches!(wb_git::commit(&standing, "again"), Err(Error::NothingToSave)));
}

#[test]
fn a_branch_name_git_would_not_accept_is_refused_before_anything_happens() {
    let sandbox = Sandbox::new("bad-name");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "first");

    for name in ["", "  ", "what if", "..", "a//b"] {
        assert!(
            matches!(
                wb_git::create_branch(&standing, name, false),
                Err(Error::BadBranchName { .. })
            ),
            "`{name}` should not be accepted"
        );
    }
    assert!(matches!(
        wb_git::create_branch(&standing, "what-if/aldric-lived", false)
            .and_then(|()| { wb_git::create_branch(&standing, "what-if/aldric-lived", false) }),
        Err(Error::BranchExists { .. })
    ));
}

// --------------------------------------------------------- the whole loop

#[test]
fn the_what_if_loop_runs_end_to_end_and_leaves_canon_where_it_was_asked_to() {
    let sandbox = Sandbox::new("loop");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "canon");
    let canon = wb_git::status(&standing).expect("status").branch.expect("on a branch");

    wb_git::create_branch(&standing, "what-if/aldric-lived", true).expect("branch and switch");
    assert_eq!(
        wb_git::status(&standing).expect("status").branch.as_deref(),
        Some("what-if/aldric-lived")
    );

    write(
        &sandbox.path().join("entities/actors/one.md"),
        "---\nid: act_one\nname: One, Alive\n---\n",
    );
    commit_all(&standing, "he lived");

    let ahead = wb_git::branches(&standing)
        .expect("branches")
        .into_iter()
        .find(|b| b.name == "what-if/aldric-lived")
        .expect("the branch");
    assert_eq!(ahead.ahead, 1, "one commit that canon does not have — what deleting would cost");
    assert_eq!(ahead.behind, 0);
    assert!(ahead.is_head);

    let merge = wb_git::merge_into(&standing, &canon).expect("fast-forward");
    assert_eq!(merge.commits, 1);
    assert_eq!(merge.into, canon);

    // The working tree did not move: the writer is still on the what-if, and canon has
    // caught up underneath them.
    assert_eq!(
        wb_git::status(&standing).expect("status").branch.as_deref(),
        Some("what-if/aldric-lived")
    );
    wb_git::switch(&standing, &canon).expect("switch to canon");
    let text = std::fs::read_to_string(sandbox.path().join("entities/actors/one.md")).unwrap();
    assert!(text.contains("One, Alive"), "canon now says what the what-if said");

    wb_git::delete_branch(&standing, "what-if/aldric-lived").expect("delete");
    assert!(wb_git::branches(&standing).expect("branches").iter().all(|b| b.name == canon));
}

#[test]
fn discarding_puts_the_folder_back_and_hands_over_a_receipt() {
    let sandbox = Sandbox::new("discard");
    seed_world(sandbox.path());
    repo_at(sandbox.path());
    let standing = Standing::of(sandbox.path());
    commit_all(&standing, "the save point");

    write(&sandbox.path().join("entities/actors/one.md"), "---\nid: act_one\nname: Wrong\n---\n");
    write(&sandbox.path().join("entities/actors/stray.md"), "---\nid: act_stray\n---\n");

    let thrown = wb_git::discard(&standing).expect("discard");
    let names: Vec<String> = thrown.iter().map(|p| p.display().to_string()).collect();
    assert_eq!(names, ["entities/actors/one.md", "entities/actors/stray.md"]);

    let text = std::fs::read_to_string(sandbox.path().join("entities/actors/one.md")).unwrap();
    assert!(text.contains("name: One"), "restored");
    assert!(!sandbox.path().join("entities/actors/stray.md").exists(), "and the new file is gone");
    assert!(wb_git::status(&standing).expect("status").dirty.is_empty());
}
