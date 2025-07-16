# pre-form (Rust TUI Commit Helper)

## Clone or install
```
git clone https://github.com/MarioMottl/pre-form
cd pre-form
cargo build --release
cargo instal --path .
```

## Create component files (minimal plain-text)
```
mkdir -p .formal-git/components
touch .formal-git/components/feat
touch .formal-git/components/fix
```
add any other types you need

## Hook into Git

### Prerequisite
Build and installed pre-form

```
pre-form install
```

Now git commit will launch the TUI and write the message into the commit file.
