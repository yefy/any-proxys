cd wasm
dirs=$(ls -l ./ |awk '/^d/ {print $NF}')
for dir in $dirs
do
  echo "start $dir"
  if [ "$dir" = "wit" ]; then
    continue
  fi
  echo "$dir:cargo component build --release"
  cd $dir
  cp ../wit/macros.rs ./src
  cp -R ../wit ./
  cargo component build --release
  cd ..
done