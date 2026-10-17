//! Kommandozeile → [`Cli`]. Bewusst ohne Parser-Crate: wenige Befehle, Entwicklerwerkzeug.

use std::{fmt, path::PathBuf};

pub const USAGE: &str = "\
todo-cli – Core + SQLite ohne UI (Entwicklerwerkzeug)

Aufruf: todo-cli [--db DATEI] [--remote ORDNER] BEFEHL …
  DB-Datei: --db, sonst $TODO_DB, sonst ./todo-dev.sqlite3
  Sync-Ordner (nur für `sync`): --remote, sonst $TODO_REMOTE

Befehle:
  lists                          Listen anzeigen
  list-add NAME                  Liste anlegen
  list-rm LISTE                  Liste löschen (Tasks wandern in die Inbox)
  add TITEL … [-l LISTE]         Task anlegen (ohne -l: Inbox)
  ls [ANSICHT] [--open]          Tasks anzeigen; ANSICHT: inbox (Standard), all, done,
                                 today, upcoming (Tagesgrenzen in UTC) oder ein Listenname
  show ID                        Task mit allen Feldern
  done ID | undo ID              abhaken / wieder öffnen
  edit ID [--title T] [--notes N] [--prio none|low|medium|high] [--due JJJJ-MM-TT|none]
  mv ID [-l LISTE] [--before ID | --after ID]
                                 verschieben (ohne Position: ans Ende)
  rm ID                          Task löschen
  sync                           mit dem Sync-Ordner abgleichen (lokales Verzeichnis als Backend)

ID: vollständige ID oder ein eindeutiges Ende davon (wie von `ls` gezeigt).
LISTE: Name (Groß-/Kleinschreibung egal) oder ID.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    /// `None` = Standardpfad (Umgebung oder Arbeitsverzeichnis).
    pub db: Option<PathBuf>,
    /// Sync-Ordner; `None` = `$TODO_REMOTE`.
    pub remote: Option<PathBuf>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    Lists,
    ListAdd {
        name: String,
    },
    ListRm {
        list: String,
    },
    Add {
        title: String,
        list: Option<String>,
    },
    Ls {
        view: String,
        open_only: bool,
    },
    Show {
        id: String,
    },
    Done {
        id: String,
        done: bool,
    },
    Edit {
        id: String,
        title: Option<String>,
        notes: Option<String>,
        prio: Option<String>,
        due: Option<String>,
    },
    Mv {
        id: String,
        list: Option<String>,
        before: Option<String>,
        after: Option<String>,
    },
    Rm {
        id: String,
    },
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError(pub String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (Hilfe: todo-cli help)", self.0)
    }
}

/// Zerlegt die Argumente (ohne Programmnamen).
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Cli, UsageError> {
    let mut args: Vec<String> = args.into_iter().collect();
    let db = take_opt(&mut args, &["--db"])?.map(PathBuf::from);
    let remote = take_opt(&mut args, &["--remote"])?.map(PathBuf::from);
    if args.is_empty() {
        return Ok(Cli {
            db,
            remote,
            command: Command::Help,
        });
    }
    let name = args.remove(0);
    let command = match name.as_str() {
        "help" | "-h" | "--help" => Command::Help,
        "lists" => {
            no_rest(&args)?;
            Command::Lists
        }
        "list-add" => Command::ListAdd {
            name: words(&args, "NAME")?,
        },
        "list-rm" => Command::ListRm {
            list: single(args, "LISTE")?,
        },
        "add" => {
            let list = take_opt(&mut args, &["-l", "--list"])?;
            Command::Add {
                title: words(&args, "TITEL")?,
                list,
            }
        }
        "ls" => {
            let open_only = take_flag(&mut args, "--open");
            let view = if args.is_empty() {
                "inbox".to_owned()
            } else {
                words(&args, "ANSICHT")?
            };
            Command::Ls { view, open_only }
        }
        "show" => Command::Show {
            id: single(args, "ID")?,
        },
        "done" | "undo" => Command::Done {
            id: single(args, "ID")?,
            done: name == "done",
        },
        "edit" => {
            let title = take_opt(&mut args, &["--title"])?;
            let notes = take_opt(&mut args, &["--notes"])?;
            let prio = take_opt(&mut args, &["--prio"])?;
            let due = take_opt(&mut args, &["--due"])?;
            let id = single(args, "ID")?;
            if title.is_none() && notes.is_none() && prio.is_none() && due.is_none() {
                return Err(UsageError("edit: nichts zu ändern".into()));
            }
            Command::Edit {
                id,
                title,
                notes,
                prio,
                due,
            }
        }
        "mv" => {
            let list = take_opt(&mut args, &["-l", "--list"])?;
            let before = take_opt(&mut args, &["--before"])?;
            let after = take_opt(&mut args, &["--after"])?;
            Command::Mv {
                id: single(args, "ID")?,
                list,
                before,
                after,
            }
        }
        "rm" => Command::Rm {
            id: single(args, "ID")?,
        },
        "sync" => {
            no_rest(&args)?;
            Command::Sync
        }
        other => return Err(UsageError(format!("unbekannter Befehl {other:?}"))),
    };
    Ok(Cli {
        db,
        remote,
        command,
    })
}

/// Entfernt `NAME WERT` aus `args`.
fn take_opt(args: &mut Vec<String>, names: &[&str]) -> Result<Option<String>, UsageError> {
    let Some(pos) = args.iter().position(|a| names.contains(&a.as_str())) else {
        return Ok(None);
    };
    if pos + 1 >= args.len() {
        return Err(UsageError(format!("{} braucht einen Wert", args[pos])));
    }
    let value = args.remove(pos + 1);
    args.remove(pos);
    Ok(Some(value))
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != name);
    args.len() != before
}

fn reject_options(args: &[String]) -> Result<(), UsageError> {
    match args.iter().find(|a| a.starts_with('-') && a.len() > 1) {
        Some(opt) => Err(UsageError(format!("unbekannte Option {opt:?}"))),
        None => Ok(()),
    }
}

fn no_rest(args: &[String]) -> Result<(), UsageError> {
    match args.first() {
        Some(extra) => Err(UsageError(format!("unerwartetes Argument {extra:?}"))),
        None => Ok(()),
    }
}

/// Genau ein Argument.
fn single(args: Vec<String>, what: &str) -> Result<String, UsageError> {
    reject_options(&args)?;
    let mut args = args.into_iter();
    let value = args
        .next()
        .ok_or_else(|| UsageError(format!("{what} fehlt")))?;
    no_rest(&args.collect::<Vec<_>>())?;
    Ok(value)
}

/// Alle restlichen Argumente als ein Text (Titel ohne Anführungszeichen).
fn words(args: &[String], what: &str) -> Result<String, UsageError> {
    reject_options(args)?;
    if args.is_empty() {
        return Err(UsageError(format!("{what} fehlt")));
    }
    Ok(args.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Result<Cli, UsageError> {
        parse(s.split_whitespace().map(str::to_owned))
    }

    fn cmd(s: &str) -> Command {
        parse_str(s).unwrap().command
    }

    #[test]
    fn sync_takes_remote_anywhere() {
        let cli = parse_str("--db a.db sync --remote ./remote").unwrap();
        assert_eq!(cli.command, Command::Sync);
        assert_eq!(cli.remote, Some(PathBuf::from("./remote")));
        assert!(parse_str("sync jetzt").is_err());
    }

    #[test]
    fn no_args_is_help() {
        assert_eq!(parse_str("").unwrap().command, Command::Help);
    }

    #[test]
    fn db_option_anywhere() {
        let cli = parse_str("ls --db /tmp/x.sqlite3 all").unwrap();
        assert_eq!(cli.db, Some(PathBuf::from("/tmp/x.sqlite3")));
        assert_eq!(
            cli.command,
            Command::Ls {
                view: "all".into(),
                open_only: false
            }
        );
    }

    #[test]
    fn add_joins_words_and_takes_list() {
        assert_eq!(
            cmd("add Milch kaufen -l Privat"),
            Command::Add {
                title: "Milch kaufen".into(),
                list: Some("Privat".into())
            }
        );
        assert!(parse_str("add").is_err());
        assert!(parse_str("add -l").is_err());
    }

    #[test]
    fn ls_defaults_to_inbox() {
        assert_eq!(
            cmd("ls --open"),
            Command::Ls {
                view: "inbox".into(),
                open_only: true
            }
        );
    }

    #[test]
    fn done_undo_mv_rm() {
        assert_eq!(
            cmd("done ab12"),
            Command::Done {
                id: "ab12".into(),
                done: true
            }
        );
        assert_eq!(
            cmd("undo ab12"),
            Command::Done {
                id: "ab12".into(),
                done: false
            }
        );
        assert_eq!(
            cmd("mv ab12 --before cd34"),
            Command::Mv {
                id: "ab12".into(),
                list: None,
                before: Some("cd34".into()),
                after: None
            }
        );
        assert_eq!(cmd("rm ab12"), Command::Rm { id: "ab12".into() });
        assert!(parse_str("rm").is_err());
        assert!(parse_str("rm a b").is_err());
        assert!(parse_str("rm --force a").is_err());
    }

    #[test]
    fn edit_needs_a_change() {
        assert!(parse_str("edit ab12").is_err());
        assert_eq!(
            cmd("edit ab12 --due none --prio high"),
            Command::Edit {
                id: "ab12".into(),
                title: None,
                notes: None,
                prio: Some("high".into()),
                due: Some("none".into())
            }
        );
    }

    #[test]
    fn unknown_command_is_error() {
        assert!(parse_str("frobnicate").is_err());
        assert!(parse_str("lists extra").is_err());
    }
}
