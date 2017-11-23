//! Exporting to CSV (compatible with Anki import).

use csv;
use failure::ResultExt;
use regex::Regex;

use contexts::ItemsInContextExt;
use errors::*;
use export::Exporter;
use srt::Subtitle;
use time::seconds_to_hhmmss_sss;

/// Attempt to guess a reasonable episode number, based on the file name.
/// Honestly, this might be a bit too clever--the original subs2srs CSV
/// format something this as part of a sort key, but we may be able to do a
/// lot better if we rethink the CSV columns we're exporting.
fn episode_prefix(file_stem: &str) -> String {
    let re = Regex::new(r"[0-9][-_.0-9]+$").unwrap();
    re.captures(file_stem)
        .map(|c| {
            let ep = c.get(0).unwrap().as_str();
            format!("{} ", ep.replace("-", ".").replace("_", "."))
        })
        .unwrap_or_else(|| "".to_owned())
}

#[test]
fn test_episode_prefix() {
    assert_eq!("01.02 ", episode_prefix("series_01_02"));
    assert_eq!("", episode_prefix("film"));
}

#[derive(Debug, Serialize)]
struct AnkiNote {
    sound: String,
    time: String,
    source: String,
    image: String,
    foreign_curr: Option<String>,
    native_curr: Option<String>,
    foreign_prev: Option<String>,
    native_prev: Option<String>,
    foreign_next: Option<String>,
    native_next: Option<String>,
}

/// Export the video and subtitles as a CSV file with accompanying media
/// files, for import into Anki.
pub fn export_csv(exporter: &mut Exporter) -> Result<()> {
    let foreign_lang = exporter.foreign().language;
    let prefix = episode_prefix(exporter.file_stem());

    // Create our CSV writer.
    let mut buffer = Vec::<u8>::new();
    {
        let mut wtr = csv::Writer::from_writer(&mut buffer);

        // Align our input files, filtering out ones with no foreign-language
        // text, because those make lousy SRS cards.  (Yes, it seems like it
        // should work, but I've seen multiple people try it now, and they're
        // maybe only 20% as effective as cards with foreign-language text, at
        // least for people below CEFRL C1.)
        let aligned: Vec<(Option<Subtitle>, Option<Subtitle>)> = exporter.align()
            .iter()
            // The double ref `&&` is thanks to `filter`'s type signature.
            .filter(|&&(ref f, _)| f.is_some())
            .cloned().collect();

        // Output each row in the CSV file.
        for ctx in aligned.items_in_context() {
            // We have a `Context<&(Option<Subtitle>, Option<Subtitle>)>`
            // containing the previous subtitle pair, the current subtitle
            // pair, and the next subtitle pair.  We want to split apart that
            // tuple and flatten any nested `Option<&Option<T>>` types into
            // `Option<&T>`.
            let foreign = ctx.map(|&(ref f, _)| f).flatten();
            let native = ctx.map(|&(_, ref n)| n).flatten();

            if let Some(curr) = foreign.curr {
                let period = curr.period.grow(1.5, 1.5);

                let image_path = exporter.schedule_image_export(period.midpoint());
                let audio_path = exporter.schedule_audio_export(foreign_lang, period);

                // Try to emulate something like the wierd sort-key column
                // generated by subs2srs without requiring the user to always
                // pass in an explicit episode number.
                let sort_key =
                    format!("{}{}", &prefix, &seconds_to_hhmmss_sss(period.begin()));

                let note = AnkiNote {
                    sound: format!("[sound:{}]", &audio_path),
                    time: sort_key,
                    source: exporter.title().to_owned(),
                    image: format!("<img src=\"{}\" />", &image_path),
                    foreign_curr: foreign.curr.map(|s| s.plain_text()),
                    native_curr: native.curr.map(|s| s.plain_text()),
                    foreign_prev: foreign.prev.map(|s| s.plain_text()),
                    native_prev: native.prev.map(|s| s.plain_text()),
                    foreign_next: foreign.next.map(|s| s.plain_text()),
                    native_next: native.next.map(|s| s.plain_text()),
                };
                wtr.serialize(&note)
                    .with_context(|_| format_err!("error serializing to RAM"))?;
            }
        }
    }

    // Write out our CSV file.
    exporter.export_data_file("cards.csv", &buffer)?;

    // Extract our media files.
    exporter.finish_exports()?;

    Ok(())
}
