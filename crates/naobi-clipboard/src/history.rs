//! The history store: every content column is a cipher blob (R-009), and the limits and retention run in
//! the same transaction as the insert (D-015). It reads and writes the kernel's `clip_entry` and
//! `clip_payload` tables.
use crate::filters::{skip_reason, Skip, DEFAULT_IGNORED};
use crate::image::{dib_to_png, ImageError};
use crate::limits::{
    Limits, INDEX_MAX_BYTES, INDEX_MAX_ENTRIES, INDEX_TTL_MS, MAX_FILE_PATHS, MAX_TEXT_BYTES,
    PREVIEW_BYTES,
};
use crate::ports::{Captured, Cipher, CipherError, Clock, Content};
use naobi_core::db::Db;
use rusqlite::{params, OptionalExtension};
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("store: {0}")]
    Db(#[from] naobi_core::db::Error),
    #[error("store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Cipher(#[from] CipherError),
    #[error("at most 100 entries can be pinned")]
    PinLimit,
}

/// What a stored copy became.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stored {
    New(i64),
    /// The same content was already the latest entry; only its time moved.
    Refreshed(i64),
}

/// What retention removed in the transaction (T-010: counts and reason, never content).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Purged {
    pub expired: usize,
    pub excess: usize,
    pub evicted: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub result: Result<Stored, Skip>,
    pub purged: Purged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Files,
}

impl Kind {
    fn column(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "image",
            Kind::Files => "files",
        }
    }

    fn format(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "png",
            Kind::Files => "filelist",
        }
    }

    fn from_column(s: &str) -> Option<Self> {
        [Kind::Text, Kind::Image, Kind::Files]
            .into_iter()
            .find(|k| k.column() == s)
    }
}

/// What the view lists. Content is fetched separately and decrypted on demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMeta {
    pub id: i64,
    pub kind: Kind,
    pub last_copied_at: i64,
    pub pinned: bool,
    pub source_exe: Option<String>,
    pub byte_size: i64,
}

/// What a capture is stored as: the payload, and a short preview for the list and the search.
struct Prepared {
    kind: Kind,
    payload: Zeroizing<Vec<u8>>,
    preview: Zeroizing<String>,
}

/// Lowercased previews of text entries, built on demand and dropped after the view has been closed
/// for `INDEX_TTL_MS` (T-014).
struct TextIndex {
    entries: Vec<(i64, Zeroizing<String>)>,
}

pub struct History {
    db: Db,
    cipher: Box<dyn Cipher>,
    clock: Box<dyn Clock>,
    limits: Limits,
    ignored: Vec<String>,
    index: Option<TextIndex>,
    view_closed_ms: Option<i64>,
}

fn clip(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

impl History {
    pub fn new(db: Db, cipher: Box<dyn Cipher>, clock: Box<dyn Clock>) -> Self {
        Self {
            db,
            cipher,
            clock,
            limits: Limits::default(),
            ignored: DEFAULT_IGNORED.iter().map(|s| (*s).to_owned()).collect(),
            index: None,
            view_closed_ms: None,
        }
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_ignored(mut self, ignored: Vec<String>) -> Self {
        self.ignored = ignored;
        self
    }

    fn prepare(c: &Captured) -> Result<Prepared, Skip> {
        match &c.content {
            Content::Text(t) => {
                if t.trim().is_empty() {
                    return Err(Skip::Empty);
                }
                if t.len() > MAX_TEXT_BYTES {
                    return Err(Skip::TooLarge);
                }
                Ok(Prepared {
                    kind: Kind::Text,
                    payload: Zeroizing::new(t.as_bytes().to_vec()),
                    preview: Zeroizing::new(clip(t, PREVIEW_BYTES).to_owned()),
                })
            }
            Content::Dib(dib) => {
                let png = dib_to_png(dib).map_err(|e| match e {
                    ImageError::Unsupported => Skip::Unsupported,
                    ImageError::TooLargeBitmap | ImageError::TooLargePng => Skip::TooLarge,
                })?;
                Ok(Prepared {
                    kind: Kind::Image,
                    preview: Zeroizing::new(format!("Image {} x {}", png.width, png.height)),
                    payload: Zeroizing::new(png.bytes),
                })
            }
            Content::Files(paths) => {
                if paths.is_empty() {
                    return Err(Skip::Empty);
                }
                let kept = &paths[..paths.len().min(MAX_FILE_PATHS)];
                let list = kept.join("\n");
                let n = kept.len();
                let first: Vec<&str> = kept
                    .iter()
                    .take(5)
                    .map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p))
                    .collect();
                Ok(Prepared {
                    kind: Kind::Files,
                    // The first line is what the list shows: how many files, then some names.
                    preview: Zeroizing::new(format!(
                        "{n} {}\n{}",
                        if n == 1 { "file" } else { "files" },
                        first.join("\n")
                    )),
                    payload: Zeroizing::new(list.into_bytes()),
                })
            }
        }
    }

    /// Stores what the clipboard held, unless a filter or a limit refuses it. Encryption happens
    /// before the transaction; the insert, the duplicate check and the retention are one transaction.
    pub fn on_change(&mut self, c: Captured) -> Result<Outcome, Error> {
        if let Some(skip) = skip_reason(&c, &self.ignored) {
            return Ok(Outcome {
                result: Err(skip),
                purged: Purged::default(),
            });
        }
        let prepared = match Self::prepare(&c) {
            Ok(p) => p,
            Err(skip) => {
                return Ok(Outcome {
                    result: Err(skip),
                    purged: Purged::default(),
                })
            }
        };
        let now = self.clock.now_ms() / 1000;
        let preview_enc = self.cipher.encrypt(prepared.preview.as_bytes())?;
        let data_enc = self.cipher.encrypt(&prepared.payload)?;
        let latest_same = self.latest_matches(&prepared)?;
        let limits = self.limits;
        let size = prepared.payload.len() as i64;
        let owner = c.owner_exe.clone();
        let (stored, purged) = self.db.transaction(|tx| {
            let stored = match latest_same {
                Some(id) => {
                    tx.execute(
                        "UPDATE clip_entry SET last_copied_at = ?1 WHERE id = ?2",
                        params![now, id],
                    )?;
                    Stored::Refreshed(id)
                }
                None => {
                    tx.execute(
                        "INSERT INTO clip_entry (kind, created_at, last_copied_at, pinned, source_exe, byte_size, preview_enc)
                         VALUES (?1, ?2, ?2, 0, ?3, ?4, ?5)",
                        params![prepared.kind.column(), now, owner, size, preview_enc],
                    )?;
                    let id = tx.last_insert_rowid();
                    tx.execute(
                        "INSERT INTO clip_payload (entry_id, format, data_enc) VALUES (?1, ?2, ?3)",
                        params![id, prepared.kind.format(), data_enc],
                    )?;
                    Stored::New(id)
                }
            };
            // Pinned entries are exempt from every purge below.
            let expired = tx.execute(
                "DELETE FROM clip_entry WHERE pinned = 0 AND last_copied_at < ?1",
                [now - limits.retention_days * 86_400],
            )?;
            let excess = tx.execute(
                "DELETE FROM clip_entry WHERE pinned = 0 AND id NOT IN
                   (SELECT id FROM clip_entry WHERE pinned = 0
                    ORDER BY last_copied_at DESC, id DESC LIMIT ?1)",
                [limits.max_entries as i64],
            )?;
            let mut evicted = 0;
            loop {
                let total: i64 =
                    tx.query_row("SELECT COALESCE(SUM(byte_size), 0) FROM clip_entry", [], |r| {
                        r.get(0)
                    })?;
                if total <= limits.store_cap_bytes {
                    break;
                }
                // The oldest unpinned entry that is not the one just stored goes first.
                let keep = match stored {
                    Stored::New(id) | Stored::Refreshed(id) => id,
                };
                let n = tx.execute(
                    "DELETE FROM clip_entry WHERE id = (SELECT id FROM clip_entry
                       WHERE pinned = 0 AND id <> ?1 ORDER BY last_copied_at ASC, id ASC LIMIT 1)",
                    [keep],
                )?;
                if n == 0 {
                    break;
                }
                evicted += n;
            }
            Ok((stored, Purged { expired, excess, evicted }))
        })?;
        // The index no longer matches the table.
        self.index = None;
        Ok(Outcome {
            result: Ok(stored),
            purged,
        })
    }

    /// The id of the latest entry when it holds exactly this content. Only the latest is compared:
    /// the store keeps no plaintext hash, and a repeated copy is nearly always the previous one.
    fn latest_matches(&self, p: &Prepared) -> Result<Option<i64>, Error> {
        let latest: Option<(i64, String, Vec<u8>)> = self
            .db
            .conn()
            .query_row(
                "SELECT e.id, e.kind, d.data_enc FROM clip_entry e
                 JOIN clip_payload d ON d.entry_id = e.id
                 ORDER BY e.last_copied_at DESC, e.id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((id, kind, blob)) = latest else {
            return Ok(None);
        };
        if Kind::from_column(&kind) != Some(p.kind) {
            return Ok(None);
        }
        let plain = self.cipher.decrypt(&blob)?;
        Ok((plain.as_slice() == p.payload.as_slice()).then_some(id))
    }

    /// The clock's time in Unix seconds, the unit the store uses.
    pub fn now_s(&self) -> i64 {
        self.clock.now_ms() / 1000
    }

    /// One entry's listing data, or None when it is gone.
    pub fn get(&self, id: i64) -> Result<Option<EntryMeta>, Error> {
        Ok(self
            .db
            .conn()
            .query_row(
                "SELECT id, kind, last_copied_at, pinned, source_exe, byte_size FROM clip_entry WHERE id = ?1",
                [id],
                |r| {
                    let kind: String = r.get(1)?;
                    Ok(EntryMeta {
                        id: r.get(0)?,
                        kind: Kind::from_column(&kind).unwrap_or(Kind::Text),
                        last_copied_at: r.get(2)?,
                        pinned: r.get::<_, i64>(3)? == 1,
                        source_exe: r.get(4)?,
                        byte_size: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// Newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<EntryMeta>, Error> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, kind, last_copied_at, pinned, source_exe, byte_size FROM clip_entry
             ORDER BY last_copied_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            let kind: String = r.get(1)?;
            Ok(EntryMeta {
                id: r.get(0)?,
                kind: Kind::from_column(&kind).unwrap_or(Kind::Text),
                last_copied_at: r.get(2)?,
                pinned: r.get::<_, i64>(3)? == 1,
                source_exe: r.get(4)?,
                byte_size: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The decrypted preview; the buffer is zeroed when dropped.
    pub fn preview(&self, id: i64) -> Result<Option<Zeroizing<String>>, Error> {
        let blob: Option<Vec<u8>> = self
            .db
            .conn()
            .query_row(
                "SELECT preview_enc FROM clip_entry WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        match blob {
            Some(b) => Ok(Some(Zeroizing::new(
                String::from_utf8_lossy(&self.cipher.decrypt(&b)?).into_owned(),
            ))),
            None => Ok(None),
        }
    }

    /// What goes back on the clipboard for entry `id`, or None when it is gone or unreadable.
    pub fn entry_payload(&self, id: i64) -> Result<Option<crate::paste::Payload>, Error> {
        use crate::paste::Payload;
        let Some(meta) = self.get(id)? else {
            return Ok(None);
        };
        let Some(data) = self.payload(id)? else {
            return Ok(None);
        };
        Ok(match meta.kind {
            Kind::Text => Some(Payload::Text(String::from_utf8_lossy(&data).into_owned())),
            Kind::Files => Some(Payload::Files(
                String::from_utf8_lossy(&data)
                    .lines()
                    .map(str::to_owned)
                    .collect(),
            )),
            Kind::Image => crate::image::png_to_dib(&data).map(|dib| Payload::Image {
                png: data.to_vec(),
                dib,
            }),
        })
    }

    /// The decrypted payload; the buffer is zeroed when dropped (T-014).
    pub fn payload(&self, id: i64) -> Result<Option<Zeroizing<Vec<u8>>>, Error> {
        let blob: Option<Vec<u8>> = self
            .db
            .conn()
            .query_row(
                "SELECT data_enc FROM clip_payload WHERE entry_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        blob.map(|b| self.cipher.decrypt(&b).map_err(Error::from))
            .transpose()
    }

    /// Ids of text entries whose first 4 KiB contain `query`, newest first. Builds the index on the
    /// first call after a change or a drop.
    pub fn search(&mut self, query: &str) -> Result<Vec<i64>, Error> {
        self.view_closed_ms = None;
        if self.index.is_none() {
            self.index = Some(self.build_index()?);
        }
        let needle = query.to_lowercase();
        Ok(self
            .index
            .iter()
            .flat_map(|i| &i.entries)
            .filter(|(_, text)| text.contains(&needle))
            .map(|(id, _)| *id)
            .collect())
    }

    fn build_index(&self) -> Result<TextIndex, Error> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, preview_enc FROM clip_entry WHERE kind = 'text'
             ORDER BY last_copied_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([INDEX_MAX_ENTRIES as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let (mut entries, mut bytes) = (Vec::new(), 0usize);
        for row in rows {
            let (id, blob) = row?;
            let text = String::from_utf8_lossy(&self.cipher.decrypt(&blob)?).to_lowercase();
            bytes += text.len();
            if bytes > INDEX_MAX_BYTES {
                break;
            }
            entries.push((id, Zeroizing::new(text)));
        }
        Ok(TextIndex { entries })
    }

    /// The history view closed: the index is dropped `INDEX_TTL_MS` later, at the next `tick`.
    pub fn view_closed(&mut self) {
        self.view_closed_ms = Some(self.clock.now_ms());
    }

    /// Drops the index once the view has been closed long enough. Call it from a timer.
    pub fn tick(&mut self) {
        if let Some(closed) = self.view_closed_ms {
            if self.clock.now_ms() - closed >= INDEX_TTL_MS {
                self.index = None;
                self.view_closed_ms = None;
            }
        }
    }

    pub fn index_loaded(&self) -> bool {
        self.index.is_some()
    }

    pub fn pin(&mut self, id: i64) -> Result<(), Error> {
        let pinned: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM clip_entry WHERE pinned = 1",
            [],
            |r| r.get(0),
        )?;
        let already: Option<i64> = self
            .db
            .conn()
            .query_row("SELECT pinned FROM clip_entry WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        if already == Some(0) && pinned as usize >= self.limits.pinned_cap {
            return Err(Error::PinLimit);
        }
        self.db
            .conn()
            .execute("UPDATE clip_entry SET pinned = 1 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn unpin(&mut self, id: i64) -> Result<(), Error> {
        self.db
            .conn()
            .execute("UPDATE clip_entry SET pinned = 0 WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn delete(&mut self, id: i64) -> Result<(), Error> {
        self.db
            .conn()
            .execute("DELETE FROM clip_entry WHERE id = ?1", [id])?;
        self.index = None;
        Ok(())
    }

    /// Removes every entry, pinned ones included (R-009).
    pub fn clear(&mut self) -> Result<usize, Error> {
        let n = self.db.conn().execute("DELETE FROM clip_entry", [])?;
        self.index = None;
        // Give the freed pages back to the file.
        self.db.conn().execute_batch("PRAGMA incremental_vacuum;")?;
        Ok(n)
    }
}
