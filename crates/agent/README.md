# tv-rs-agent

Fleet reconciliation and automated recovery engine for collections of smart televisions and commercial displays.

## Highlights

- **Desired-State Engine**: Compares target display states against observed states.
- **Graduated Remediation Ladder**: Progresses from soft commands to power cycles and quarantine.
- **Lease-Aware Coordination**: Honors `MaintenanceLease` (`ObserveAndRelay` / `DisconnectAndYield`) during maintenance or technician overrides.
- **RAG-Guided Healing**: Integrates with `tv-rs-rag` ledger to select empirically successful remediation actions per vendor and model.
- **Audit Logging**: Structured JSONL audit records for every probe, decision, and remedy.
