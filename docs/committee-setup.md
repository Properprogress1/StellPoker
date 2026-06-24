# TACEO coNoir MPC Committee Setup Guide

This guide documents the setup process for a 3-node MPC (Multi-Party Computation) committee utilizing TACEO's coNoir framework for replicated 3-party secret sharing (REP3).

---

## 1. Overview of the DKG Process

In a traditional Multi-Party Computation committee (such as threshold ECDSA or BLS), a **Distributed Key Generation (DKG)** ceremony is used to collaboratively generate a shared public key and distributed private key shares.

However, the TACEO coNoir REP3 proving system:
1. **Uses Direct Secret Sharing**: The private values (the card deck and player salts) are split directly into replicated secret shares by the input owner (the dealer/node) using `co-noir split-input`.
2. **Does Not Use a Threshold Key Pair**: Proving is computed collaboratively using replicated secret sharing directly over BN254. Therefore, a cryptographic DKG ceremony is **not implemented** and **not required** for the core proof generation.

Instead, the "setup ceremony" for a coNoir committee consists of:
- **Transport Security Setup**: Generating TLS certificates and private keys to establish secure, mutually authenticated TCP connections (mTLS) between the three nodes.
- **On-chain Node Registration**: Initializing the `CommitteeRegistry` contract, funding the nodes, and registering them on-chain with their network endpoints.
- **Epoch Activation**: Packing the registered members into an active committee epoch on-chain, which authorizes them to submit proofs.

```
                  [ mTLS Network Setup ]
             Node 0 ◄═══════ mTLS ═══════► Node 1
               ▲                            ▲
               ╚════════════ mTLS ══════════╝
                            Node 2

                  [ On-chain Registration ]
       Node 0/1/2 ──(Register & Stake)──► CommitteeRegistry
                                                │
       Admin ──────────(Create Epoch)───────────┘
```

---

## 2. Prerequisites

The following software is required to set up and run the committee:

- **Rust & Cargo** (v1.79 or newer)
- **Stellar CLI** (compatible with Soroban Protocol 25/26)
- **Docker & Docker Compose** (for running the local Stellar standalone network)
- **OpenSSL** (for TLS key/certificate generation)
- **curl** (for Friendbot funding)
- **co-noir** (TACEO coNoir CLI tool)
  ```bash
  cargo install --git https://github.com/TaceoLabs/co-snarks --branch main co-noir
  ```

---

## 3. Required Binaries and Configuration

A coNoir participant runs two main components:

1. **`co-noir` CLI**: Invoked by the node service to perform merge, witness generation, and proving operations.
2. **`mpc-node` Service**: An HTTP wrapper (built in Axum) that orchestrates the invocation of `co-noir` commands and coordinates share exchanges with peers.

### Node Configuration Schema
Each participant requires a TOML configuration specifying its network parameters and peers:

```toml
[network]
my_id = 0                      # Party ID (0, 1, or 2)
bind_addr = "0.0.0.0:10000"    # Local port to bind for co-noir p2p connections
key_path = "path/to/key.der"   # Node's private key (DER format)
max_frame_length = 469762056   # Max TCP frame length (bytes)

[[network.parties]]
id = 0
dns_name = "127.0.0.1:10000"   # Endpoint address for Party 0
cert_path = "path/to/cert0.der" # Public certificate for Party 0

[[network.parties]]
id = 1
dns_name = "127.0.0.1:10001"   # Endpoint address for Party 1
cert_path = "path/to/cert1.der" # Public certificate for Party 1

[[network.parties]]
id = 2
dns_name = "127.0.0.1:10002"   # Endpoint address for Party 2
cert_path = "path/to/cert2.der" # Public certificate for Party 2
```

---

## 4. Preparing Three REP3 Nodes

To prepare the nodes, you must generate unique TLS credentials and distribute peer configurations.

### Generating TLS Credentials
For secure peer-to-peer transport authentication, each node needs a private key and a self-signed certificate in DER format. 

Use OpenSSL to generate prime256v1 (ECDSA) keys and certificates:

```bash
# Generate private key (SEC1 EC private key in PEM format)
openssl ecparam -name prime256v1 -genkey -noout -out key.pem

# Convert private key to DER format (PKCS#8/SEC1 format)
openssl ec -in key.pem -outform DER -out key.der

# Create OpenSSL configuration for certificate generation
cat > cert.conf <<EOF
[req]
distinguished_name = req_distinguished_name
x509_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = party0

[v3_req]
basicConstraints = critical, CA:TRUE
keyUsage = keyEncipherment, dataEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names

[alt_names]
IP.1 = 127.0.0.1
DNS.1 = localhost
EOF

# Generate self-signed certificate in DER format
openssl req -new -x509 -key key.pem -sha256 -days 365 -config cert.conf -outform DER -out cert.der
```

Repeat this process for each node, changing the `CN = partyX` in the configuration.

---

## 5. Step-by-Step DKG Ceremony (Setup and Registration)

This section details how to perform the full local setup and register the committee members on-chain.

### Step 1: Start Stellar Local Network
Ensure the Stellar container is running:
```bash
docker-compose up -d soroban
```

### Step 2: Deploy Contracts
Deploy the Soroban smart contracts (which outputs `.env.local` containing contract addresses):
```bash
./scripts/deploy-local.sh
```

### Step 3: Source Contract Environment
```bash
source .env.local
```

### Step 4: Run the Setup Script
Run the automated script to generate TLS credentials, write configs, and register nodes on-chain:
```bash
./scripts/setup-dkg.sh
```

### Step 5: Start the Nodes
Start the three MPC node services:
```bash
# Node 0
NODE_ID=0 PORT=8101 PARTY_CONFIG="services/node/config/local/party_0.toml" cargo run -p mpc-node &
# Node 1
NODE_ID=1 PORT=8102 PARTY_CONFIG="services/node/config/local/party_1.toml" cargo run -p mpc-node &
# Node 2
NODE_ID=2 PORT=8103 PARTY_CONFIG="services/node/config/local/party_2.toml" cargo run -p mpc-node &
```

### Step 6: Start the Coordinator
```bash
CIRCUIT_DIR="./circuits" CRS_DIR="./crs" BIND_ADDR="0.0.0.0:8080" cargo run -p coordinator
```

---

## 6. Expected Outputs

Successful execution of the setup process yields the following artifacts:

### Generated Key Material
- **`services/node/data/key0.der`**, **`key1.der`**, **`key2.der`**: Binary elliptic-curve private keys (P-256).
- **`services/node/data/cert0.der`**, **`cert1.der`**, **`cert2.der`**: Self-signed mTLS public certificates.

### Node Configuration TOMLs
- **`services/node/config/local/party_0.toml`**, **`party_1.toml`**, **`party_2.toml`**: Configurations linking peer endpoints to the generated certificate paths.

### On-chain State in `CommitteeRegistry`
- Members registered with their Stellar address, stake balance, and network endpoint.
- Active epoch (Epoch 1) containing all 3 member addresses, with a quorum threshold of `2`.

---

## 7. How to Verify Successful Completion

You can verify that the committee is properly set up using two checks.

### On-Chain Epoch Verification
Query the current active epoch from the `CommitteeRegistry` contract:
```bash
stellar contract invoke \
  --id "$COMMITTEE_REGISTRY_CONTRACT" \
  --source committee-local \
  --network local \
  -- get_current_epoch
```
**Expected Terminal Output:**
```json
{
  "epoch_id": 1,
  "members": [
    "GB...",
    "GD...",
    "GC..."
  ],
  "threshold": 2,
  "start_ledger": 125,
  "end_ledger": 0
}
```

### Coordinator Health Check
Once all node services are running, ping the coordinator's committee status endpoint:
```bash
curl -s http://localhost:8080/api/committee/status
```
**Expected Response:**
```json
{"nodes":3,"healthy":[true,true,true],"status":"active"}
```

---

## 8. Common Failure Cases and Troubleshooting

### 1. `ResourceLimitExceeded` during member registration
- **Cause**: Soroban contract invocations exceeded the default transaction resource limits on the local network.
- **Solution**: The automated script invokes commands with `--instruction-leeway` to scale limits. If encountered manually, append `--instruction-leeway 500000000` to the CLI invocation.

### 2. TLS Handshake Failures between Nodes
- **Cause**: The SAN (Subject Alternative Name) in the certificate does not match the peer address specified in `party_X.toml` (e.g. using `localhost` vs `127.0.0.1`), or certificates were generated without `basicConstraints = CA:TRUE`.
- **Solution**: Re-run `scripts/setup-dkg.sh` to generate compliant certificates. Ensure the node's `dns_name` in the TOML matches the SAN IP or DNS entry.

### 3. Node Registration Fails with "insufficient stake"
- **Cause**: The node identity account has a balance lower than the required minimum stake (100 XLM / 1,000,000,000 stroops).
- **Solution**: Ensure Friendbot funding succeeded. You can check the balance using:
  ```bash
  stellar keys balance node0-local --network local
  ```

---

## 9. Security Recommendations

### Node Isolation
- **No Shared Environments**: Run each node on separate physical or virtual servers. Never host multiple committee participants on the same virtual machine or hypervisor to prevent single-point-of-failure compromises.
- **Dedicated Accounts**: Each node operator must control a distinct, secure Stellar signing key. Do not share administrative or participant keys.

### Secure Networking
- **mTLS Enforcement**: Ensure `co-noir` is configured to mandate mutual TLS (mTLS) for all peer-to-peer TCP channels. Reject all non-encrypted or unauthenticated peer requests.
- **Firewall Rules (IP Whitelisting)**: Restrict access to the node P2P ports (e.g., `10000-10002`). Configure firewalls (security groups, `iptables`) to strictly allow connections only from the specific IP addresses of the other two committee members.
- **Separate API Binding**: Bind the node API wrapper (`mpc-node`) to localhost or a private network interface accessible only by the coordinator. Never expose the Axum HTTP ports (`8101-8103`) to the public internet.

### Key Backup Strategy
- **Stellar Key Custody**: Node operators must securely back up their Stellar signing secret keys. Use encrypted secret storage (e.g., Vault, AWS KMS) or hardware security modules (HSMs).
- **TLS Key Backups**: Keep offline backups of the TLS private keys (`key*.der`) in an encrypted vault. If a TLS key is lost, the node will be unable to establish connections, leading to committee downtime and potential slashing.

### Secret Handling
- **No Hardcoded Secrets**: Never hardcode Stellar private keys or TLS keys in configurations or container images. Load keys dynamically via environment variables or secure file mounts (`chmod 400`).
- **RAM Disks**: If possible, mount node data directories (containing `key.der` and temporary ACIR proof assets) on a `tmpfs` (RAM disk) to prevent writing private cryptographic material to non-volatile storage.

### Filesystem Permissions
- **Restrict Private Keys**: Ensure the node's private TLS key (`key*.der`) has permissions set to `600` (read/write only by owner) or `400` (read-only).
- **Non-Root Execution**: Configure Docker containers or system services to run the `mpc-node` and `co-noir` binaries as a non-privileged system user, never as `root`.

### Rotation Policy
- **TLS Certificate Rotation**: Periodically rotate TLS certificates (e.g., every 90 days). You can update the peer certificates in each node's config TOML and perform a rolling restart without affecting service availability.
- **Stellar Key Rotation**: If a node operator's Stellar key is suspected to be compromised, immediately deregister the node from the `CommitteeRegistry` and register a new identity.

### Development vs Production Recommendations
- **Certificates**: 
  - *Development*: Self-signed certificates generated via local OpenSSL scripts.
  - *Production*: Certificates signed by an internal PKI or trusted CA, with explicit hostname verification enabled.
- **Networking**: 
  - *Development*: Bind to `127.0.0.1` and run nodes on the same host.
  - *Production*: Publicly addressable static IPs, protected by enterprise-grade firewalls, routing traffic over VPC peering or private overlays.
- **Authentication**:
  - *Development*: Coordinator allows signature verification bypass in tests (`ALLOW_INSECURE_DEV_AUTH=1`).
  - *Production*: Strict cryptographic authentication of all coordinator-to-node requests via Ed25519 signatures.

### Recovery Considerations
- **Admin Epoch Transitions**: In the event of a node crash, network split, or compromised private key, the `CommitteeRegistry` owner can transition to a new epoch by calling `create_epoch` and omitting the faulty node.
- **Graceful Slashing Protection**: Set `timeout_ledgers` conservatively in the table configuration to allow nodes ample time to recover and compute proofs before being reported for slashing.
