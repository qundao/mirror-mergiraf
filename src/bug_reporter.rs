use core::str;
use std::{
    env,
    fs::File,
    io::{self, Write},
    path::Path,
};

use rand::distr::{Alphanumeric, SampleString};
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{attempts::AttemptsCache, git::extract_all_revisions_from_git};

/// Creates an archive containing files necessary to reproduce a faulty merge
pub fn report_bug(attempt_id_or_path: &str) -> Result<(), String> {
    let attempts_cache = AttemptsCache::new(None, None)?;
    let archive_name = if let Ok(attempt) = attempts_cache.parse_attempt_id(attempt_id_or_path) {
        let path_base = attempt.path("Base");
        let path_left = attempt.path("Left");
        let path_right = attempt.path("Right");
        let best_merge_id = attempt.best_merge_id()?;
        let path_result = attempt.path(&best_merge_id);
        create_archive(
            attempt.file_name,
            Some(&path_base),
            Some(&path_left),
            Some(&path_right),
            &path_result,
        )
        .map_err(|err| format!("error while creating report archive: {err}"))?
    } else {
        // it could be a file with conflicts
        let path = Path::new(attempt_id_or_path);
        if !path.is_file() {
            return Err("Invalid path or merge attempt id provided".to_owned());
        }
        let current_working_dir = env::current_dir().expect("Invalid current directory");
        let crate::git::GitTempFiles { base, left, right } =
            extract_all_revisions_from_git(&current_working_dir, path)?;

        create_archive(
            path.file_name()
                .and_then(|os_str| os_str.to_str())
                .unwrap_or("no_filename"),
            base.as_ref().map(super::git::GitTempFile::path),
            left.as_ref().map(super::git::GitTempFile::path),
            right.as_ref().map(super::git::GitTempFile::path),
            path,
        )
        .map_err(|err| format!("error while creating report archive: {err}"))?
    };

    println!("\
Bug report archive created:

{archive_name}

Please submit it to https://codeberg.org/mergiraf/mergiraf/issues if you are happy with its contents being published,
or reach out privately to a contributor if not.
Thank you for helping Mergiraf improve!");
    Ok(())
}

fn create_archive(
    filename: &str,
    path_base: Option<&Path>,
    path_left: Option<&Path>,
    path_right: Option<&Path>,
    path_result: &Path,
) -> Result<String, io::Error> {
    let extension = filename
        .rsplit_once('.')
        .map_or("no_ext", |(_stem, ext)| ext);
    let archive_base_name = format!(
        "mergiraf_report_{}",
        Alphanumeric.sample_string(&mut rand::rng(), 8)
    );
    let archive_name = format!("{archive_base_name}.zip");
    let file_desc = File::create(&archive_name)?;
    let mut zip = ZipWriter::new(file_desc);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file(format!("{archive_base_name}/Base.{extension}"), options)?;
    if let Some(path) = path_base {
        io::copy(&mut File::open(path)?, &mut zip)?;
    };

    zip.start_file(format!("{archive_base_name}/Left.{extension}"), options)?;
    if let Some(path) = path_left {
        io::copy(&mut File::open(path)?, &mut zip)?;
    };

    zip.start_file(format!("{archive_base_name}/Right.{extension}"), options)?;
    if let Some(path) = path_right {
        io::copy(&mut File::open(path)?, &mut zip)?;
    };

    zip.start_file(format!("{archive_base_name}/Result.{extension}"), options)?;
    let mut right_file = File::open(path_result)?;
    io::copy(&mut right_file, &mut zip)?;

    zip.start_file(format!("{archive_base_name}/filename.txt"), options)?;
    zip.write_all(filename.as_bytes())?;

    zip.start_file(format!("{archive_base_name}/version.txt"), options)?;
    zip.write_all(env!("CARGO_PKG_VERSION").as_bytes())?;

    zip.finish()?;
    Ok(archive_name)
}
