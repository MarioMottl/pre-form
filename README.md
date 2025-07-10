# pre-form (Rust TUI Commit Helper)

## Clone or install
```
git clone <repo-url> pre-form
cd pre-form
cargo build --release
```

## Create component files (minimal plain-text)
```
mkdir -p .formal-git/components
touch .formal-git/components/feat
touch .formal-git/components/fix
```
add any other types you need

## Run manually
```
target/release/pre-form
```
Use arrow keys to pick a type, TAB to switch fields, type your message, ENTER to finish.

## Hook into Git

Copy the hook script into your repo:
```
mkdir -p .git/hooks
cat > .git/hooks/prepare-commit-msg << 'EOF'
#!/bin/sh
# pre-form Git hook: generates commit message via TUI
if [ -z "$2" ]; then
  pre-form "$1"
fi
EOF
chmod +x .git/hooks/prepare-commit-msg
```

Now git commit will launch the TUI and write the message into the commit file.
