use core::str;
use std::{
    env,
    fs::File,
    io::{self, Write},
    path::Path,
};

use rand::distributions::{Alphanumeric, DistString};
use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{attempts::AttemptsCache, git::extract_revision_from_git, pcs::Revision};

/// Creates an archive containing files necessary to reproduce a faulty merge
pub fn report_bug(attempt_id_or_path: String) -> Result<(), String> {
    let attempts_cache = AttemptsCache::new(None, None)?;
    let archive_name = if let Ok(attempt) = attempts_cache.parse_attempt_id(&attempt_id_or_path) {
        let path_base = attempt.path("Base");
        let path_left = attempt.path("Left");
        let path_right = attempt.path("Right");
        let best_merge_id = attempt.best_merge_id()?;
        let path_result = attempt.path(&best_merge_id);
        create_archive(
            attempt.file_name,
            &path_base,
            &path_left,
            &path_right,
            &path_result,
        )
        .map_err(|err| format!("error while creating report archive: {err}"))?
    } else {
        // it could be a file with conflicts
        let path = Path::new(&attempt_id_or_path);
        if !path.is_file() {
            return Err("Invalid path or merge attempt id provided".to_owned());
        }
        let current_working_dir = env::current_dir().expect("Invalid current directory");
        let temp_file_base = extract_revision_from_git(&current_working_dir, path, Revision::Base)?;
        let temp_file_left = extract_revision_from_git(&current_working_dir, path, Revision::Left)?;
        let temp_file_right =
            extract_revision_from_git(&current_working_dir, path, Revision::Right)?;

        create_archive(
            path.file_name()
                .and_then(|os_str| os_str.to_str())
                .unwrap_or("no_filename"),
            temp_file_base.path(),
            temp_file_left.path(),
            temp_file_right.path(),
            path,
        )
        .map_err(|err| format!("error while creating report archive: {err}"))?
    };

    println!("Bug report archive created:\n");
    println!("{archive_name}");
    println!("\nPlease submit it to https://codeberg.org/mergiraf/mergiraf/issues if you are happy with its contents being published,");
    println!("or reach out privately to a contributor if not.");
    println!("Thank you for helping Mergiraf improve!");
    Ok(())
}

fn create_archive(
    filename: &str,
    path_base: &Path,
    path_left: &Path,
    path_right: &Path,
    path_result: &Path,
) -> Result<String, io::Error> {
    let extension = filename.split('.').last().unwrap_or("no_ext");
    let archive_base_name = format!(
        "mergiraf_report_{}",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 8)
    );
    let archive_name = format!("{archive_base_name}.zip");
    let file_desc = File::create(archive_name.clone())?;
    let mut zip = ZipWriter::new(file_desc);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file(format!("{archive_base_name}/Base.{extension}"), options)?;
    let mut base_file = File::open(path_base)?;
    io::copy(&mut base_file, &mut zip)?;

    zip.start_file(format!("{archive_base_name}/Left.{extension}"), options)?;
    let mut left_file = File::open(path_left)?;
    io::copy(&mut left_file, &mut zip)?;

    zip.start_file(format!("{archive_base_name}/Right.{extension}"), options)?;
    let mut right_file = File::open(path_right)?;
    io::copy(&mut right_file, &mut zip)?;

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
