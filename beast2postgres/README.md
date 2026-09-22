# beast2postgres


A Rust program to store ADSB messages in postgres.

- consumes messages from the server started with `make dump1090`
* batches them and sends them to postgres using the COPY protocol using the [`batch-copy`](github.com/perrygeo/batch-copy) library.

Since a good antenna and SDR module in a busy location might get thousands of message per second, individual INSERTs are too costly. Use batch COPY instead.

## Usage

Launch a postgres instance and set `DATABASE_URL`. If you don't have one running, use the docker version in `./infra`

```
cd infra
docker compose up database
export DATABASE_URL="postgresql://postgres:password@localhost:5432/postgres"
cd ..
```

Then run the main program, which will loop indefinitely. Use `ctrl-c` to stop.

```
$ cargo run --release
    Finished `release` profile [optimized] target(s) in 0.09s
     Running `/home/mperry/projects/sdr/beast2postgres/target/release/beast2postgres --host x1`
Connecting to Database...
Listening for ADS-B messages...
Messages: 15186, Time: 1m39s, Rate: 153.4msg/s
```


