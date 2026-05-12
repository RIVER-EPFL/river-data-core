#!/usr/bin/env Rscript
#
# Verify that each r_reference/functions/*.R file is a byte-exact
# extraction from the portal source file.
#
# Usage:
#   Rscript r_reference/verify_integrity.R <path_to_calculation_functions.R>

args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 1) {
  stop("Usage: Rscript verify_integrity.R <path_to_calculation_functions.R>")
}

source_path <- args[1]
if (!file.exists(source_path)) stop("Source file not found: ", source_path)

source_lines <- readLines(source_path, warn = FALSE)

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
  if (length(line_header) == 0) {
    cat(fname, ": FAIL (no Lines: header found)\n")
    failed <- failed + 1
    next
  }

  range_str <- sub("^# Lines: ", "", line_header[1])
  parts <- as.integer(strsplit(range_str, "-")[[1]])
  start_line <- parts[1]
  end_line <- parts[2]

  extracted <- source_lines[start_line:end_line]

  blank_idx <- grep("^#", ref_lines)
  header_end <- max(blank_idx[blank_idx <= 8], 0)
  if (header_end > 0 && header_end < length(ref_lines)) {
    first_non_blank <- header_end + 1
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
