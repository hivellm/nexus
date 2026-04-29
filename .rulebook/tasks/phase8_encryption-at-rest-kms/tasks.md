## 1. AWS KMS
- [ ] 1.1 Add aws-sdk-kms dep, gate behind kms-aws feature
- [ ] 1.2 Implement AwsKmsKeyProvider (DEK pattern: KMS-wrapped data key)
- [ ] 1.3 Integration test against localstack

## 2. GCP KMS
- [ ] 2.1 Add google-cloud-kms dep, gate behind kms-gcp feature
- [ ] 2.2 Implement GcpKmsKeyProvider
- [ ] 2.3 Integration test against the GCP emulator

## 3. HashiCorp Vault
- [ ] 3.1 Add vaultrs dep, gate behind kms-vault feature
- [ ] 3.2 Implement VaultKeyProvider (transit secret engine)
- [ ] 3.3 Integration test against `vault dev`

## 4. Operator config
- [ ] 4.1 Wire --kms-provider flag + per-provider config keys
- [ ] 4.2 Document each provider's required env vars

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update or create documentation covering the implementation
- [ ] 5.2 Write tests covering the new behavior
- [ ] 5.3 Run tests and confirm they pass
