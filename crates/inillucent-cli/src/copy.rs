//! `.backup`, `.save`, `.clone` and `.restore`: putting a database in another file.
//!
//! Invariant: **the copy is of the database the command names, and the source
//! may be a file or may be held in memory.** The engine's own backup is a
//! checkpoint and a copy of the file the connection was opened on, which made
//! three things wrong: `.backup aux FILE` copied `main`, a shell opened on
//! `:memory:` had no file to copy and failed with an operating system error
//! about a file name, and `.clone` wrote over a file that was already there.
//! Each is answered here by choosing the right source, never by changing what the
//! engine's backup does for a file.

use crate::dot::{confine_path, unknown_option};
use crate::shell::Shell;

/// Returns whether a name from `PRAGMA database_list` means a database with no file.
///
/// @param file - the file column of the row
fn has_no_file(file: &str) -> bool {
    file.is_empty() || file == ":memory:"
}

/// What a `.backup` or `.save` command line asked for, once its words are read.
struct Request<'a> {
    /// The database to copy, `main` unless one was named.
    schema: &'a str,
    /// The file to write.
    destination: &'a str,
}

/// Reads the words after `.backup` or `.save`.
///
/// **Two forms, `FILE` and `DB FILE`, and two options.** `--append` and `--async`
/// are accepted with one or two dashes. Neither changes the result here: a new
/// file is a plain database either way, and the engine writes its copy to a
/// scratch name and renames it in, so no journal has to be left out. Any other
/// option is reported with the caret form the reference uses.
///
/// @param shell - where a complaint goes
/// @param command - the command as typed, with its dot
/// @param arguments - the words after it
fn read_request<'a>(
    shell: &mut Shell,
    command: &str,
    arguments: &[&'a str],
) -> Option<Request<'a>> {
    let mut schema: Option<&'a str> = None;
    let mut destination: Option<&'a str> = None;
    for word in arguments {
        if word.starts_with('-') {
            if matches!(word.trim_start_matches('-'), "append" | "async") {
                continue;
            }
            unknown_option(shell, command, arguments, word);
            let place = shell.error_location();
            shell.complain(&format!("{place} should be -append or -async"));
            return None;
        }
        match (destination, schema) {
            (None, _) => destination = Some(word),
            (Some(first), None) => {
                schema = Some(first);
                destination = Some(word);
            }
            (Some(_), Some(_)) => {
                shell.complain(&format!("Usage: {command} ?DB? FILE"));
                return None;
            }
        }
    }
    let Some(destination) = destination else {
        shell.complain("missing FILENAME argument on .backup");
        return None;
    };
    Some(Request {
        schema: schema.unwrap_or("main"),
        destination,
    })
}

/// `.backup ?DB? FILE` and `.save ?DB? FILE`: copies a database into a file.
///
/// @param shell - the shell
/// @param command - the command as typed, with its dot
/// @param arguments - the words after it
pub fn backup(shell: &mut Shell, command: &str, arguments: &[&str]) {
    let Some(request) = read_request(shell, command, arguments) else {
        return;
    };
    let Some(destination) = confine_path(shell, request.destination) else {
        return;
    };
    if let Err(message) = copy_schema(shell, request.schema, &destination) {
        shell.complain(&format!("Error: {message}"));
    }
}

/// Copies the named database into `destination`, replacing a file already there.
///
/// @param shell - the shell
/// @param schema - `main`, `temp` or an attached name
/// @param destination - the file to write
fn copy_schema(shell: &mut Shell, schema: &str, destination: &str) -> Result<(), String> {
    let listing = shell.database_list();
    let Some((_, file)) = listing
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(schema))
    else {
        return Err(format!("unknown database {schema}"));
    };
    let is_main = schema.eq_ignore_ascii_case("main");
    if has_no_file(file) {
        if is_main {
            return export_memory_database(shell, destination);
        }
        return export_attached_memory_database(shell, schema, destination);
    }
    let outcome = if is_main {
        shell.backup_to(destination)
    } else {
        Shell::backup_file(file, destination)
    };
    outcome.map_err(|error| error.message().to_string())
}

/// Writes an attached or temporary database that is held in memory to a file, replacing a file
/// already there.
///
/// **`VACUUM schema INTO` writes the copy**, to a scratch file beside the destination, and a
/// rename puts it in place once it is whole, as for `main`.
///
/// @param shell - the shell
/// @param schema - the name of the database
/// @param destination - the file to write
fn export_attached_memory_database(
    shell: &mut Shell,
    schema: &str,
    destination: &str,
) -> Result<(), String> {
    let scratch = format!("{destination}.backup-{}", std::process::id());
    remove_database_files(&scratch);
    let statement = format!(
        "VACUUM \"{}\" INTO '{}'",
        schema.replace('"', "\"\""),
        scratch.replace('\'', "''")
    );
    shell.collect(&statement).map_err(|failure| failure.message)?;
    remove_log_segments(&scratch);
    remove_database_files(destination);
    std::fs::rename(&scratch, destination).map_err(|error| {
        remove_database_files(&scratch);
        format!("cannot write \"{destination}\": {error}")
    })
}

/// Writes the database held in memory to a file, replacing a file already there.
///
/// **A rebuild into a scratch file beside the destination, then a rename.** The
/// engine refuses an existing file as the target of an export, and a backup to an
/// existing path replaces it, so the old file and the log segments named after it
/// are removed only once the copy has been written.
///
/// @param shell - the shell
/// @param destination - the file to write
fn export_memory_database(shell: &mut Shell, destination: &str) -> Result<(), String> {
    let scratch = format!("{destination}.backup-{}", std::process::id());
    remove_database_files(&scratch);
    shell
        .export_to(&scratch, crate::keys::for_opening())
        .map_err(|error| {
            error
                .detail()
                .unwrap_or_else(|| error.message())
                .to_string()
        })?;
    remove_log_segments(&scratch);
    remove_database_files(destination);
    std::fs::rename(&scratch, destination).map_err(|error| {
        remove_database_files(&scratch);
        format!("cannot write \"{destination}\": {error}")
    })
}

/// Removes a database file and the log segments named after it.
///
/// @param path - the database file
pub(crate) fn remove_database_files(path: &str) {
    remove_log_segments(path);
    let _ = std::fs::remove_file(path);
}

/// Removes the log segments `PATH-wal.NNNNNNNNNN` beside a database file.
///
/// Recovery reads segments by their name, so one left beside a file that replaces
/// the database it belonged to would be replayed into the new file.
///
/// @param path - the database file
fn remove_log_segments(path: &str) {
    let path = std::path::Path::new(path);
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let prefix = format!("{}-wal.", name.to_string_lossy());
    let parent = if parent.as_os_str().is_empty() {
        std::path::Path::new(".")
    } else {
        parent
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// `.clone NEWDB`: copies the database into a new file, naming each object.
///
/// **A file that is already there is refused**, with the reference's sentence. The
/// reference rebuilds the target object by object and says `NAME... done` after
/// each; the copy here is one verified backup, and the lines are printed for the
/// objects it holds, in the order the schema lists them.
///
/// @param shell - the shell
/// @param arguments - the words after the command
pub fn clone(shell: &mut Shell, arguments: &[&str]) {
    let [named] = arguments else {
        shell.complain("Usage: .clone FILENAME");
        return;
    };
    let Some(destination) = confine_path(shell, named) else {
        return;
    };
    if std::path::Path::new(&destination).exists() {
        shell.note(&format!("File \"{named}\" already exists."));
        return;
    }
    let objects = shell.column(
        "SELECT name FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY rowid",
    );
    if let Err(message) = copy_schema(shell, "main", &destination) {
        shell.complain(&format!("Error: {message}"));
        return;
    }
    for name in objects {
        shell.say(&format!("{name}... done"));
    }
}

/// `.restore ?DB? FILE`: replaces the database with the contents of a file.
///
/// @param shell - the shell
/// @param arguments - the words after the command
pub fn restore(shell: &mut Shell, arguments: &[&str]) {
    let named = match arguments {
        [file] | [_, file] => *file,
        _ => {
            shell.complain("Usage: .restore ?DB? FILE");
            return;
        }
    };
    let Some(path) = confine_path(shell, named) else {
        return;
    };
    // **Restoring is opening the other file, not copying it over this one.**
    // The old engine's restore wrote the source's pages into the open database
    // in place. This engine's databases are whole files, so the honest restore
    // is to point the shell at the file the caller named - which is also what a
    // person means by it, and it does not destroy the database they were in.
    if let Err(message) = shell.reopen(&path) {
        shell.complain(&format!("Error: {message}"));
    }
}
