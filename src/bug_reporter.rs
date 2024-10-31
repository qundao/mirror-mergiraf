use core::{str};
use std::{
    env,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use rand::distributions::{Alphanumeric, DistString};
use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{attempts::AttemptsCache, pcs::Revision};

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
            &attempt.file_name,
            &path_base,
            &path_left,
            &path_right,
            &path_result,
        )
        .map_err(|err| format!("error while creating report archive: {}", err.to_string()))?
    } else {
        // it could be a file with conflicts
        let path = Path::new(&attempt_id_or_path);
        if !path.is_file() {
            return Err("Invalid path or merge attempt id provided".to_owned());
        }
        let temp_file_base = extract_side(path, Revision::Base)?;
        let temp_file_left = extract_side(path, Revision::Left)?;
        let temp_file_right = extract_side(path, Revision::Right)?;

        let archive_name = create_archive(
            path.file_name()
                .and_then(|os_str| os_str.to_str())
                .unwrap_or("no_filename"),
            &temp_file_base,
            &temp_file_left,
            &temp_file_right,
            path,
        )
        .map_err(|err| format!("error while creating report archive: {}", err.to_string()))?;

        fs::remove_file(&temp_file_base).map_err(|err| err.to_string())?;
        fs::remove_file(&temp_file_left).map_err(|err| err.to_string())?;
        fs::remove_file(&temp_file_right).map_err(|err| err.to_string())?;
        archive_name
    };

    println!("Bug report archive created:\n");
    println!("{}", archive_name);
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
    let extension = filename.split(".").last().unwrap_or("no_ext");
    let archive_base_name = format!(
        "mergiraf_report_{}",
        Alphanumeric.sample_string(&mut rand::thread_rng(), 8)
    );
    let archive_name = format!("{}.zip", archive_base_name);
    let file_desc = File::create(archive_name.clone())?;
    let mut zip = ZipWriter::new(file_desc);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file(
        format!("{}/Base.{}", archive_base_name, extension),
        options.clone(),
    )?;
    let mut base_file = File::open(&path_base)?;
    io::copy(&mut base_file, &mut zip)?;

    zip.start_file(
        format!("{}/Left.{}", archive_base_name, extension),
        options.clone(),
    )?;
    let mut left_file = File::open(&path_left)?;
    io::copy(&mut left_file, &mut zip)?;

    zip.start_file(
        format!("{}/Right.{}", archive_base_name, extension),
        options.clone(),
    )?;
    let mut right_file = File::open(&path_right)?;
    io::copy(&mut right_file, &mut zip)?;

    zip.start_file(
        format!("{}/Result.{}", archive_base_name, extension),
        options.clone(),
    )?;
    let mut right_file = File::open(&path_result)?;
    io::copy(&mut right_file, &mut zip)?;

    zip.start_file(
        format!("{}/filename.txt", archive_base_name),
        options.clone(),
    )?;
    zip.write_all(filename.as_bytes())?;

    zip.start_file(
        format!("{}/version.txt", archive_base_name),
        options.clone(),
    )?;
    zip.write_all(env!("CARGO_PKG_VERSION").as_bytes())?;

    zip.finish()?;
    Ok(archive_name)
}

fn extract_side(path: &Path, revision: Revision) -> Result<PathBuf, String> {
    let mut command = Command::new("git");
    command
        .arg("checkout-index")
        .arg("--temp")
        .arg(match revision {
            Revision::Base => "--stage=1",
            Revision::Left => "--stage=2",
            Revision::Right => "--stage=3",
        })
        .arg(path)
        .output()
        .map_err(|err| err.to_string())
        .and_then(|output| {
            if !output.status.success() {
                let error_str = str::from_utf8(&output.stderr).map_err(|err| err.to_string())?;
                return Err(format!(
                    "error while retrieving {} revision for {}:\n{}",
                    revision,
                    path.display(),
                    error_str
                ));
            }
            let output_str = str::from_utf8(&output.stdout).map_err(|err| err.to_string())?;
            let temp_file_path = output_str.split_ascii_whitespace().next().ok_or_else(|| {
                format!(
                    "git did not return a temporary file path for {} revision of {}",
                    revision,
                    path.display()
                )
            })?;
            Ok(PathBuf::from(temp_file_path))
        })
}
