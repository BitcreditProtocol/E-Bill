curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh
chmod +x install.sh
./install.sh
echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zshrc && source ~/.zshrc