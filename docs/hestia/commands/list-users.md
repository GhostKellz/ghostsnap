# `ghostsnap hestia list-users` - List Users Command

List HestiaCP users available for backup.

---

## Synopsis

```bash
ghostsnap hestia list-users [OPTIONS]
```

---

## Description

The `list-users` command discovers and displays HestiaCP users on the system.

**Discovery Method**:
- Reads `/usr/local/hestia/data/users/` directory
- Each file represents a user (e.g., `admin`, `alice`)
- Can optionally show detailed user information

**Use Cases**:
- ✅ Discover users before backup
- ✅ Verify user existence
- ✅ Audit HestiaCP accounts
- ✅ Generate user lists for scripts

---

## Options

### Optional

#### `--detailed`

Show detailed information for each user.

**Alias**: `-d`

**Default**: `false` (simple list)

**Example**:
```bash
ghostsnap hestia list-users --detailed
```

---

## Examples

### Basic Examples

#### Simple List

```bash
ghostsnap hestia list-users
```

**Output**:
```
admin
alice
bob
charlie
dev-tester
prod-web
prod-api
staging-app
test-user1
```

**Use Case**: Quick user enumeration for backup planning.

---

#### Detailed List

```bash
ghostsnap hestia list-users --detailed
```

**Output**:
```
Username: admin
  Name: Administrator
  Email: admin@example.com
  Package: default
  Web Domains: 5
  DNS Domains: 5
  Mail Domains: 3
  Databases: 10
  Cron Jobs: 2
  Disk Usage: 2.5G
  Bandwidth Usage: 15.2G
  Status: active
  Suspended: no

Username: alice
  Name: Alice Smith
  Email: alice@example.com
  Package: pro
  Web Domains: 3
  DNS Domains: 3
  Mail Domains: 2
  Databases: 5
  Cron Jobs: 1
  Disk Usage: 1.2G
  Bandwidth Usage: 8.5G
  Status: active
  Suspended: no

...
```

---

### Advanced Examples

#### Count Users

```bash
ghostsnap hestia list-users | wc -l
```

**Output**:
```
9
```

---

#### Filter Users

```bash
# List production users only
ghostsnap hestia list-users | grep "^prod-"
```

**Output**:
```
prod-web
prod-api
```

---

#### Export to File

```bash
# Simple list
ghostsnap hestia list-users > users.txt

# Detailed list
ghostsnap hestia list-users --detailed > users-detailed.txt
```

---

#### Use in Backup Script

```bash
#!/bin/bash
# Backup all users dynamically

USERS=$(ghostsnap hestia list-users)

for USER in $USERS; do
  echo "Backing up: $USER"
  ghostsnap hestia backup --user "$USER" --repository /var/ghostsnap/repo
done
```

---

#### Check User Exists

```bash
#!/bin/bash
USERNAME="admin"

if ghostsnap hestia list-users | grep -q "^${USERNAME}$"; then
  echo "✓ User $USERNAME exists"
else
  echo "✗ User $USERNAME not found"
  exit 1
fi
```

---

## Behavior Details

### Discovery Process

```
1. Check if HestiaCP is installed
   Path: /usr/local/hestia/data/users/
   
2. List files in directory
   
3. For each file:
   - Filename = username
   - (If --detailed) Parse user config
   
4. Output users (sorted alphabetically)
```

---

### User Config Format

HestiaCP stores user data in:
```
/usr/local/hestia/data/users/<username>/user.conf
```

**Example** (`/usr/local/hestia/data/users/admin/user.conf`):
```ini
USER='admin'
NAME='Administrator'
EMAIL='admin@example.com'
PACKAGE='default'
WEB_DOMAINS='5'
DNS_DOMAINS='5'
MAIL_DOMAINS='3'
DATABASES='10'
CRON_JOBS='2'
U_DISK='2560000'
U_BANDWIDTH='15728640'
STATUS='active'
SUSPENDED='no'
TIME='2024-01-15'
DATE='2024-01-15 10:30:00'
```

**Parsed Fields** (with `--detailed`):
- `USER`: Username
- `NAME`: Full name
- `EMAIL`: Email address
- `PACKAGE`: Hosting package
- `WEB_DOMAINS`: Number of web domains
- `DNS_DOMAINS`: Number of DNS zones
- `MAIL_DOMAINS`: Number of mail domains
- `DATABASES`: Number of databases
- `CRON_JOBS`: Number of cron jobs
- `U_DISK`: Disk usage (KB)
- `U_BANDWIDTH`: Bandwidth usage (KB)
- `STATUS`: Account status (active/suspended)
- `SUSPENDED`: Suspension flag (yes/no)

---

### Sorting

Users are sorted **alphabetically** by username:

```
admin
alice
bob
charlie
dev-tester
```

---

## Error Handling

### Common Errors

#### HestiaCP Not Installed

```
Error: HestiaCP data directory not found: /usr/local/hestia/data/users/
```

**Solution**: Install HestiaCP:
```bash
wget https://raw.githubusercontent.com/hestiacp/hestiacp/release/install/hst-install.sh
sudo bash hst-install.sh
```

---

#### Permission Denied

```
Error: Permission denied (os error 13)
```

**Solution**: Run with `sudo` (only needed for `--detailed`):
```bash
sudo ghostsnap hestia list-users --detailed
```

**Note**: Simple list usually works without sudo.

---

#### No Users Found

```
No HestiaCP users found
```

**Possible Causes**:
- Fresh HestiaCP installation
- All users deleted
- Wrong HestiaCP installation path

**Solution**: Create a user:
```bash
sudo v-add-user admin password admin@example.com
```

---

## Output Format

### Simple List

**Format**: One username per line, sorted alphabetically.

```
admin
alice
bob
```

**Character Set**: ASCII alphanumeric + hyphen + underscore
**Max Length**: 16 characters (HestiaCP limit)

---

### Detailed List

**Format**: Multi-line per user, blank line separator.

```
Username: <username>
  Name: <full_name>
  Email: <email>
  Package: <package>
  Web Domains: <count>
  DNS Domains: <count>
  Mail Domains: <count>
  Databases: <count>
  Cron Jobs: <count>
  Disk Usage: <size>
  Bandwidth Usage: <size>
  Status: <active|suspended>
  Suspended: <yes|no>

Username: <next_user>
  ...
```

**Size Format**: Human-readable (MB, GB, TB)

---

## Performance

### Benchmarks

| Users | Simple List | Detailed List |
|-------|-------------|---------------|
| 10 | <10ms | ~50ms |
| 100 | ~50ms | ~500ms |
| 1000 | ~200ms | ~5s |

**Note**: Detailed list requires reading and parsing each user config file.

---

### Optimization

For large user counts (100+):

```bash
# Simple list (fast)
ghostsnap hestia list-users

# Detailed info for specific user
ghostsnap hestia user-info admin
```

---

## Scripting Examples

### Backup Only Active Users

```bash
#!/bin/bash
# Backup only active (non-suspended) users

USERS=$(ghostsnap hestia list-users --detailed | \
        grep -B12 "Status: active" | \
        grep "Username:" | \
        awk '{print $2}')

for USER in $USERS; do
  echo "Backing up: $USER"
  ghostsnap hestia backup --user "$USER" --repository /var/ghostsnap/repo
done
```

---

### Generate User Report

```bash
#!/bin/bash
# Generate CSV report

echo "Username,Email,Disk Usage,Bandwidth Usage,Status" > users.csv

ghostsnap hestia list-users --detailed | \
  awk '
    /Username:/ {user=$2}
    /Email:/ {email=$2}
    /Disk Usage:/ {disk=$3}
    /Bandwidth Usage:/ {bw=$3}
    /Status:/ {status=$2; print user","email","disk","bw","status}
  ' >> users.csv

echo "Report generated: users.csv"
```

---

### Validate User Before Backup

```bash
#!/bin/bash
validate_user() {
  local username=$1
  
  if ! ghostsnap hestia list-users | grep -q "^${username}$"; then
    echo "Error: User '$username' not found"
    echo "Available users:"
    ghostsnap hestia list-users
    return 1
  fi
  
  return 0
}

# Usage
if validate_user "admin"; then
  ghostsnap hestia backup --user admin --repository /var/ghostsnap/repo
fi
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success - users listed |
| `1` | General error (HestiaCP not found) |
| `77` | Permission denied |

---

## See Also

- **[user-info](user-info.md)** - Detailed info for specific user
- **[backup](backup.md)** - Backup users
- **[list-backups](list-backups.md)** - View backups

---

**Back to**: [Commands Overview](README.md) | [HestiaCP Integration](../README.md)
