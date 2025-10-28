#!/bin/bash
###############################################################################
# Ghostsnap WordPress Backup Script
#
# Backs up WordPress site (files + database) to Ghostsnap repository
#
# Usage:
#   ./wordpress-backup.sh [options]
#
# Options:
#   -s, --site-root DIR       WordPress root directory (default: auto-detect)
#   -u, --db-user USER        Database username (default: from wp-config.php)
#   -d, --db-name NAME        Database name (default: from wp-config.php)
#   -p, --db-pass PASS        Database password (default: from wp-config.php)
#   -r, --repo PATH           Ghostsnap repository path (default: $GHOSTSNAP_REPO)
#   -t, --tag TAG             Additional backup tag
#   -h, --help                Show this help message
#
# Environment Variables:
#   GHOSTSNAP_PASSWORD        Repository password (required)
#   GHOSTSNAP_REPO            Repository path (required if -r not specified)
#
###############################################################################

set -euo pipefail

# Default configuration
SITE_ROOT=""
DB_USER=""
DB_NAME=""
DB_PASS=""
REPO_PATH="${GHOSTSNAP_REPO:-}"
EXTRA_TAGS=()
GHOSTSNAP_BIN="ghostsnap"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Logging functions
log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
    exit 1
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Show usage
usage() {
    head -n 25 "$0" | tail -n +3 | sed 's/^# \?//'
    exit 0
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--site-root)
            SITE_ROOT="$2"
            shift 2
            ;;
        -u|--db-user)
            DB_USER="$2"
            shift 2
            ;;
        -d|--db-name)
            DB_NAME="$2"
            shift 2
            ;;
        -p|--db-pass)
            DB_PASS="$2"
            shift 2
            ;;
        -r|--repo)
            REPO_PATH="$2"
            shift 2
            ;;
        -t|--tag)
            EXTRA_TAGS+=("$2")
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            error "Unknown option: $1. Use -h for help."
            ;;
    esac
done

# Validate environment
if [ -z "${GHOSTSNAP_PASSWORD:-}" ]; then
    error "GHOSTSNAP_PASSWORD environment variable is required"
fi

if [ -z "$REPO_PATH" ]; then
    error "Repository path required: set GHOSTSNAP_REPO or use -r option"
fi

# Auto-detect WordPress root if not specified
if [ -z "$SITE_ROOT" ]; then
    info "Auto-detecting WordPress root..."

    # Common HestiaCP locations
    if [ -d "/home" ]; then
        for user_dir in /home/*/web/*/public_html; do
            if [ -f "$user_dir/wp-config.php" ]; then
                SITE_ROOT="$user_dir"
                log "Found WordPress at: $SITE_ROOT"
                break
            fi
        done
    fi

    if [ -z "$SITE_ROOT" ]; then
        error "Could not auto-detect WordPress root. Use -s option."
    fi
fi

# Validate WordPress installation
if [ ! -f "$SITE_ROOT/wp-config.php" ]; then
    error "wp-config.php not found in $SITE_ROOT"
fi

# Extract database credentials from wp-config.php if not provided
if [ -z "$DB_NAME" ] || [ -z "$DB_USER" ] || [ -z "$DB_PASS" ]; then
    info "Extracting database credentials from wp-config.php..."

    WP_CONFIG="$SITE_ROOT/wp-config.php"

    if [ -z "$DB_NAME" ]; then
        DB_NAME=$(grep "define.*DB_NAME" "$WP_CONFIG" | cut -d "'" -f 4)
        info "Database name: $DB_NAME"
    fi

    if [ -z "$DB_USER" ]; then
        DB_USER=$(grep "define.*DB_USER" "$WP_CONFIG" | cut -d "'" -f 4)
        info "Database user: $DB_USER"
    fi

    if [ -z "$DB_PASS" ]; then
        DB_PASS=$(grep "define.*DB_PASSWORD" "$WP_CONFIG" | cut -d "'" -f 4)
    fi
fi

# Validate database credentials
if [ -z "$DB_NAME" ] || [ -z "$DB_USER" ]; then
    error "Could not determine database credentials"
fi

# Create temporary directory for database dump
BACKUP_DIR=$(mktemp -d)
trap "rm -rf $BACKUP_DIR" EXIT

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DB_FILE="$BACKUP_DIR/wordpress-db-$TIMESTAMP.sql"

log "==================== WordPress Backup Started ===================="
log "Site: $SITE_ROOT"
log "Database: $DB_NAME"
log "Repository: $REPO_PATH"
log "=================================================================="

# Step 1: Backup database
log "Step 1/3: Backing up WordPress database..."

if [ -n "$DB_PASS" ]; then
    MYSQL_PWD="$DB_PASS" mysqldump -u "$DB_USER" "$DB_NAME" > "$DB_FILE" 2>/dev/null || error "Database dump failed"
else
    mysqldump -u "$DB_USER" "$DB_NAME" > "$DB_FILE" 2>/dev/null || error "Database dump failed"
fi

DB_SIZE=$(du -h "$DB_FILE" | cut -f1)
log "Database exported: $DB_SIZE"

# Compress database
gzip "$DB_FILE"
DB_FILE="${DB_FILE}.gz"
DB_SIZE_COMPRESSED=$(du -h "$DB_FILE" | cut -f1)
log "Database compressed: $DB_SIZE_COMPRESSED"

# Backup database to ghostsnap
TAGS=("--tag" "wordpress" "--tag" "database" "--tag" "$(date +%Y-%m-%d)")
for tag in "${EXTRA_TAGS[@]}"; do
    TAGS+=("--tag" "$tag")
done

if GHOSTSNAP_REPO="$REPO_PATH" "$GHOSTSNAP_BIN" backup "$DB_FILE" "${TAGS[@]}" >/dev/null; then
    log "✅ Database backup successful"
else
    error "Database backup to Ghostsnap failed"
fi

# Step 2: Backup WordPress files
log "Step 2/3: Backing up WordPress files..."

FILE_TAGS=("--tag" "wordpress" "--tag" "files" "--tag" "$(date +%Y-%m-%d)")
for tag in "${EXTRA_TAGS[@]}"; do
    FILE_TAGS+=("--tag" "$tag")
done

# Common WordPress excludes
EXCLUDES=(
    "--exclude" "wp-content/cache/*"
    "--exclude" "wp-content/uploads/cache/*"
    "--exclude" "wp-content/w3tc-cache/*"
    "--exclude" "wp-content/wp-rocket-cache/*"
    "--exclude" "wp-content/wflogs/*"
    "--exclude" "wp-content/backup-*"
    "--exclude" "wp-content/backups-*"
    "--exclude" "wp-content/updraft/*"
    "--exclude" "*.log"
    "--exclude" "error_log"
    "--exclude" ".git"
    "--exclude" ".svn"
    "--exclude" "node_modules"
    "--exclude" ".DS_Store"
)

SITE_SIZE=$(du -sh "$SITE_ROOT" 2>/dev/null | cut -f1)
log "Site size: $SITE_SIZE"

if GHOSTSNAP_REPO="$REPO_PATH" "$GHOSTSNAP_BIN" backup "$SITE_ROOT" "${FILE_TAGS[@]}" "${EXCLUDES[@]}" >/dev/null; then
    log "✅ Files backup successful"
else
    error "Files backup to Ghostsnap failed"
fi

# Step 3: Show summary
log "Step 3/3: Generating backup summary..."

echo ""
log "==================== Backup Summary ======================="
GHOSTSNAP_REPO="$REPO_PATH" "$GHOSTSNAP_BIN" snapshots | head -6
log "==========================================================="
echo ""

log "✅ WordPress backup completed successfully!"

exit 0
