cd wasm
dirs=$(ls -l ./ |awk '/^d/ {print $NF}')

cp -R ./http-demo/wit ./
cd ./http-demo
cargo component build --release
cd ..

for dir in $dirs
do
  echo "start $dir"
  if [ "$dir" = "wit" ]; then
    continue
  fi
  if [ "$dir" = "http-demo" ]; then
    continue
  fi
  echo "$dir:cargo component build --release"
  cd $dir
  cp -R ../http-demo/wit ./
  cp ../http-demo/src/macros.rs ./src
  cp ../http-demo/src/lib.rs ./src
  cp -R ../http-demo/src/util ./src
  cargo component build --release
  cd ..
done