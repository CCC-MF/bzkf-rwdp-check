/*
 * This file is part of bzkf-rwdp-check
 *
 * Copyright (C) 2024 Comprehensive Cancer Center Mainfranken and contributors.
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, write to the Free Software Foundation, Inc.,
 * 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
 */

use std::error::Error;
use std::path::Path;

use clap::Parser;
use console::{style, Term};
use csv::Writer;
use itertools::Itertools;

use crate::cli::{Cli, SubCommand};
use crate::common::{Check, DiffRecord, Icd10GroupSize};
use crate::database::DatabaseSource;

mod cli;
mod common;
mod database;
mod opal;
mod resources;

fn request_password_if_none(password: Option<String>) -> String {
    if let Some(password) = password {
        password
    } else {
        let password = dialoguer::Password::new()
            .with_prompt("Password")
            .interact()
            .unwrap_or_default();
        let _ = Term::stdout().clear_last_lines(1);
        password
    }
}

fn sanitize_year(year: String) -> String {
    if year.len() == 4 {
        year
    } else {
        format!("2{:0>3}", year)
    }
}

fn print_items(items: &[Icd10GroupSize]) {
    let term = Term::stdout();
    let _ = term.write_line(
        &style("Anzahl der Conditions nach ICD-10-Gruppe")
            .yellow()
            .to_string(),
    );
    items.iter().for_each(|item| {
        let _ = term.write_line(&format!("{:<20}={:>6}", item.name, item.size));
    });
    let sum: usize = items
        .iter()
        .filter(|item| item.name != "Other")
        .map(|item| item.size)
        .sum();
    let _ = term.write_line(&style("─".repeat(27)).dim().to_string());
    let _ = term.write_line(
        &style(format!("{:<20}={:>6}", "Summe (C**.*/D**.*)", sum))
            .dim()
            .to_string(),
    );
    let sum: usize = items.iter().map(|item| item.size).sum();
    let _ = term.write_line(
        &style(format!("{:<20}={:>6}", "Gesamtsumme", sum))
            .dim()
            .to_string(),
    );
    let _ = term.write_line(&style("─".repeat(27)).dim().to_string());
}

fn main() -> Result<(), Box<dyn Error>> {
    let term = Term::stdout();

    match Cli::parse().cmd {
        SubCommand::OpalFile { file } => {
            let items = opal::OpalCsvFile::check(Path::new(&file))
                .map_err(|_e| "Kann Datei nicht lesen")?;

            print_items(&items);
        }
        SubCommand::Database {
            database,
            host,
            password,
            port,
            user,
            year,
            ignore_exports_since,
        } => {
            let password = request_password_if_none(password);
            let year = sanitize_year(year);

            let _ = term.write_line(
                &style(format!("Warte auf Daten für das Diagnosejahr {}...", year))
                    .blue()
                    .to_string(),
            );

            let db = DatabaseSource::new(&database, &host, &password, port, &user);
            let items = db
                .check(&year, &ignore_exports_since.unwrap_or("9999-12-31".into()))
                .map_err(|_e| "Fehler bei Zugriff auf die Datenbank")?;

            let _ = term.clear_last_lines(1);

            print_items(&items);
        }
        SubCommand::Export {
            pat_id,
            database,
            host,
            password,
            port,
            user,
            output,
            year,
            ignore_exports_since,
        } => {
            let password = request_password_if_none(password);
            let year = sanitize_year(year);

            let _ = term.write_line(
                &style(format!("Warte auf Daten für das Diagnosejahr {}...", year))
                    .blue()
                    .to_string(),
            );

            let db = DatabaseSource::new(&database, &host, &password, port, &user);
            let items = db
                .export(
                    &year,
                    &ignore_exports_since.unwrap_or("9999-12-31".into()),
                    pat_id,
                )
                .map_err(|_e| "Fehler bei Zugriff auf die Datenbank")?;

            let _ = term.clear_last_lines(1);

            let mut writer = Writer::from_path(Path::new(&output)).expect("writeable file");

            items
                .iter()
                .for_each(|item| writer.serialize(item).unwrap());

            let _ = term.write_line(
                &style(format!(
                    "{} Conditions für das Jahr {} in Datei '{}' exportiert",
                    items.len(),
                    year,
                    output
                ))
                .green()
                .to_string(),
            );
        }
        SubCommand::Compare {
            pat_id,
            database,
            host,
            password,
            port,
            user,
            file,
            year,
            ignore_exports_since,
        } => {
            let password = request_password_if_none(password);
            let year = sanitize_year(year);

            let _ = term.write_line(
                &style(format!("Warte auf Daten für das Diagnosejahr {}...", year))
                    .blue()
                    .to_string(),
            );

            let db = DatabaseSource::new(&database, &host, &password, port, &user);
            let db_items = db
                .export(
                    &year,
                    &ignore_exports_since.unwrap_or("9999-12-31".into()),
                    pat_id,
                )
                .map_err(|_e| "Fehler bei Zugriff auf die Datenbank")?;

            let _ = term.clear_last_lines(1);

            let csv_items = opal::OpalCsvFile::export(Path::new(&file))
                .map_err(|_e| "Kann Datei nicht lesen")?;

            let mut not_in_csv = db_items
                .iter()
                .filter(|db_item| {
                    !csv_items
                        .iter()
                        .map(|csv_item| &csv_item.condition_id)
                        .contains(&db_item.condition_id)
                })
                .collect::<Vec<_>>();

            let _ = term.write_line(
                &style(format!(
                    "{} Conditions aus der Datenbank für das Jahr {} - aber nicht in Datei '{}'",
                    not_in_csv.len(),
                    year,
                    file
                ))
                .green()
                .to_string(),
            );

            let _ = term.write_line(&format!(
                "{:<64}   {:<10}   {:<5}   {:<5}   {}",
                "Condition-ID", "Datum", "ICD10", "", "PAT-ID"
            ));

            not_in_csv.sort_by_key(|item| item.condition_id.to_string());

            not_in_csv
                .iter()
                .for_each(|item| match Check::is_relevant(&item.icd_10_code) {
                    true => {
                        let _ = term.write_line(&format!(
                            "{:<64}   {:<10}   {:<5}   {:<5}   {}",
                            item.condition_id,
                            item.diagnosis_date,
                            style(&item.icd_10_code).bold().red(),
                            "",
                            match &item.pat_id {
                                Some(ref pat_id) => pat_id.to_string(),
                                _ => "".to_string(),
                            }
                        ));
                    }
                    false => {
                        let _ = term.write_line(&format!(
                            "{:<64}   {:<10}   {:<5}   {:<5}   {}",
                            item.condition_id,
                            item.diagnosis_date,
                            item.icd_10_code,
                            "",
                            match &item.pat_id {
                                Some(ref pat_id) => pat_id.to_string(),
                                _ => "".to_string(),
                            }
                        ));
                    }
                });

            let mut not_in_db = csv_items
                .iter()
                .filter(|csv_item| {
                    !db_items
                        .iter()
                        .map(|db_item| &db_item.condition_id)
                        .contains(&csv_item.condition_id)
                })
                .collect::<Vec<_>>();

            let _ = term.write_line(
                &style(format!(
                    "{} Conditions aus Datei '{}' - aber nicht in der Datenbank für das Jahr {}",
                    not_in_db.len(),
                    file,
                    year
                ))
                .green()
                .to_string(),
            );

            let _ = term.write_line(&format!(
                "{:<64}   {:<10}   {:<5}",
                "Condition-ID", "Datum", "ICD10"
            ));

            not_in_db.sort_by_key(|item| item.condition_id.to_string());

            not_in_db
                .iter()
                .for_each(|item| match Check::is_relevant(&item.icd_10_code) {
                    true => {
                        let _ = term.write_line(&format!(
                            "{:<64}   {:<10}   {:<5}",
                            item.condition_id,
                            item.diagnosis_date,
                            style(&item.icd_10_code).bold().red()
                        ));
                    }
                    false => {
                        let _ = term.write_line(&format!(
                            "{:<64}   {:<10}   {:<5}",
                            item.condition_id, item.diagnosis_date, item.icd_10_code
                        ));
                    }
                });

            let mut icd10diff = db_items
                .iter()
                .filter(|db_item| {
                    csv_items
                        .iter()
                        .map(|db_item| &db_item.condition_id)
                        .contains(&db_item.condition_id)
                })
                .filter(|db_item| {
                    !csv_items
                        .iter()
                        .map(|csv_item| {
                            format!("{}-{}", csv_item.condition_id, csv_item.icd_10_code)
                        })
                        .contains(&format!("{}-{}", db_item.condition_id, db_item.icd_10_code))
                })
                .map(|db_item| DiffRecord {
                    pat_id: db_item.pat_id.as_ref().map(|pat_id| pat_id.to_string()),
                    condition_id: db_item.condition_id.to_string(),
                    diagnosis_date: db_item.diagnosis_date.to_string(),
                    csv_icd10_code: db_item.icd_10_code.to_string(),
                    db_icd10_code: csv_items
                        .iter()
                        .filter(|csv_item| csv_item.condition_id == db_item.condition_id)
                        .collect_vec()
                        .first()
                        .unwrap()
                        .icd_10_code
                        .to_string(),
                })
                .collect::<Vec<_>>();

            let _ = term.write_line(
                &style(format!(
                    "{} Conditions mit Unterschied im ICD10-Code",
                    icd10diff.len()
                ))
                .green()
                .to_string(),
            );

            icd10diff.sort_by_key(|item| item.condition_id.to_string());

            let _ = term.write_line(&format!(
                "{:<64}   {:<10}   {:<5}   {:<5}   {}",
                "Condition-ID", "Datum", "CSV", "DB", "PAT-ID"
            ));

            icd10diff.iter().for_each(|item| {
                let _ = term.write_line(&format!(
                    "{:<64}   {:<10}   {:<5}   {:<5}   {}",
                    item.condition_id,
                    item.diagnosis_date,
                    match Check::is_relevant(&item.csv_icd10_code) {
                        true => style(format!("{:<5}", item.csv_icd10_code)).bold().red(),
                        _ => style(format!("{:<5}", item.csv_icd10_code)),
                    },
                    match Check::is_relevant(&item.db_icd10_code) {
                        true => style(format!("{:<5}", item.db_icd10_code)).bold().red(),
                        _ => style(format!("{:<5}", item.db_icd10_code)),
                    },
                    match &item.pat_id {
                        Some(ref pat_id) => pat_id.to_string(),
                        _ => "".to_string(),
                    }
                ));
            });
        }
    }

    Ok(())
}
