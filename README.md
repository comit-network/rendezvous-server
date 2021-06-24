# Standalone Rendezvous Server

A standalone libp2p [rendezvous server](https://github.com/libp2p/specs/tree/master/rendezvous) binary.

## Usage

Run the `rendezvous_server`:

```
rendezvous-server --secret-key 12345678123456781234567812345678 --port 8888
```

The `register_once` example program can be used to register a namespace with a `rendezvous server`

```
register_once --port 8889 --secret-key 11111111111111111111111111111111 --rendezvous-peer_id 12D3KooWHj6UHDzDibbThLy3e3mAMSrneuohnMa3NnbmCYgHyGRg --rendezvous-addr /ip4/121.118.112.55/tcp/3020/p2p/12D3KoowHj6UHDzDibbThLy2e33aMDrneuoxnMa3NnbmCYgHyYgh
```