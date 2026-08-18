#!/usr/bin/env Rscript
#
# Verify that each r_reference/functions/*.R file is a byte-exact
# extraction from the portal source file named in its "# File:" header.
#
# Usage:
#   Rscript r_reference/verify_integrity.R <path_to_cnet-data-portal_repo>
#   Rscript r_reference/verify_integrity.R <path_to_calculation_functions.R>
#
# A file argument only covers references extracted from that file; pass the
# portal repo root to verify every reference.

args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 1) {
  stop("Usage: Rscript verify_integrity.R <portal_repo_root_or_source_file>")
}

source_path <- args[1]
if (!file.exists(source_path)) stop("Source path not found: ", source_path)

# Resolve a reference's "# File:" header to source lines. With a single-file
# argument, only references whose File basename matches are checkable.
source_cache <- new.env()
load_source <- function(rel_path) {
  if (!is.null(source_cache[[rel_path]])) return(source_cache[[rel_path]])
  path <- if (dir.exists(source_path)) file.path(source_path, rel_path) else {
    if (basename(rel_path) == basename(source_path)) source_path else NA_character_
  }
  if (is.na(path) || !file.exists(path)) return(NULL)
  lines <- readLines(path, warn = FALSE)
  source_cache[[rel_path]] <- lines
  lines
}

script_dir <- dirname(sub("--file=", "", commandArgs(trailingOnly = FALSE)[grep("--file=", commandArgs(trailingOnly = FALSE))]))
if (length(script_dir) == 0) script_dir <- "r_reference"
func_dir <- file.path(script_dir, "functions")

files <- list.files(func_dir, pattern = "\\.R$", full.names = TRUE)
if (length(files) == 0) stop("No .R files found in ", func_dir)

passed <- 0
failed <- 0

for (fpath in sort(files)) {
  fname <- basename(fpath)
  ref_lines <- readLines(fpath, warn = FALSE)

  line_header <- grep("^# Lines:", ref_lines, value = TRUE)
  file_header <- grep("^# File:", ref_lines, value = TRUE)
  if (length(line_header) == 0 || length(file_header) == 0) {
    cat(fname, ": FAIL (no Lines:/File: header found)\n")
    failed <- failed + 1
    next
  }

  source_lines <- load_source(sub("^# File: ", "", file_header[1]))
  if (is.null(source_lines)) {
    cat(fname, ": SKIP (source ", sub("^# File: ", "", file_header[1]),
        " not reachable from ", source_path, ")\n", sep = "")
    next
  }

  range_str <- sub("^# Lines: ", "", line_header[1])
  parts <- as.integer(strsplit(range_str, "-")[[1]])
  start_line <- parts[1]
  end_line <- parts[2]

  extracted <- source_lines[start_line:end_line]

# The header is the initial comment block up to the first blank line
  first_blank <- which(trimws(ref_lines) == "")[1]
  if (!is.na(first_blank) && all(grepl("^#", ref_lines[seq_len(first_blank - 1)]))) {
    first_non_blank <- first_blank
    while (first_non_blank <= length(ref_lines) && trimws(ref_lines[first_non_blank]) == "") {
      first_non_blank <- first_non_blank + 1
    }
    body <- ref_lines[first_non_blank:length(ref_lines)]
  } else {
    body <- ref_lines
  }

  # Trim trailing blank lines from both
  while (length(extracted) > 0 && trimws(extracted[length(extracted)]) == "") {
    extracted <- extracted[-length(extracted)]
  }
  while (length(body) > 0 && trimws(body[length(body)]) == "") {
    body <- body[-length(body)]
  }

  if (length(body) != length(extracted)) {
    cat(fname, " (lines ", start_line, "-", end_line, "): FAIL — ",
        length(body), " body lines vs ", length(extracted), " source lines\n", sep = "")
    failed <- failed + 1
    next
  }

  mismatches <- which(body != extracted)
  if (length(mismatches) > 0) {
    cat(fname, " (lines ", start_line, "-", end_line, "): FAIL — ",
        length(mismatches), " lines differ (first at line ", mismatches[1], ")\n", sep = "")
    cat("  ref: ", body[mismatches[1]], "\n")
    cat("  src: ", extracted[mismatches[1]], "\n")
    failed <- failed + 1
  } else {
    cat(fname, " (lines ", start_line, "-", end_line, "): PASS (",
        length(body), " lines match)\n", sep = "")
    passed <- passed + 1
  }
}

cat("\n", passed, "/", passed + failed, " PASSED\n", sep = "")
if (failed > 0) {
  cat(failed, " FAILED\n")
  quit(status = 1)
}
