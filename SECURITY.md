# Security Policy

## Reporting Security Vulnerabilities

If you discover a security vulnerability in this project, please report it to us privately before disclosing it publicly.

### Reporting Process

- **Email**: security@simplytokenized.com
- **PGP Key**: Available upon request for encrypted communications
- **Response Time**: We aim to acknowledge receipt within 48 hours and provide a detailed response within 7 days

### What to Include

Please include the following information in your report:
- Type of vulnerability (e.g., buffer overflow, SQL injection, cross-site scripting)
- Steps to reproduce the vulnerability
- Potential impact of the vulnerability
- Any proof-of-concept code or screenshots (if applicable)

## Security Scope

The following are considered in-scope for security assessments:
- Smart contract source code in `/token/`, `/crowdsale/`, and `/payout/` directories
- Contract deployment and interaction scripts
- Build and compilation processes

The following are considered out-of-scope:
- Third-party dependencies and libraries
- Infrastructure and deployment environments
- Social engineering attacks
- Denial of service attacks against test networks

## Supported Versions

Only the latest version of this project receives security updates. Users are strongly encouraged to keep their implementations up to date.

## Security Best Practices

### For Developers
- Always audit smart contract code before deployment
- Use formal verification tools when available
- Follow the [Soroban Security Guidelines](https://soroban.stellar.org/docs/security)
- Implement proper access controls and input validation

### For Users
- Only interact with contracts from verified sources
- Review contract source code before interacting
- Use hardware wallets when possible
- Never share private keys or seed phrases

## Smart Contract Security Considerations

### Access Control
- Ensure proper ownership patterns are implemented
- Validate caller permissions for sensitive functions
- Implement multi-signature requirements for critical operations

### Input Validation
- Validate all external inputs
- Check for integer overflow/underflow
- Validate address formats and bounds

### Financial Safety
- Implement proper reentrancy protection
- Use safe math operations for financial calculations
- Consider slippage and price impact in token swaps

### Contract-Specific Security Assessment

#### emergency_reset_sale (Crowdsale Contract)
**Purpose**: Emergency recovery function to reset the crowdsale contract from corrupted or bad state transitions (e.g., stuck sale due to configuration errors, invalid TOTAL_SOLD values).

**Risks**:
- Can be abused to reset TOTAL_SOLD to zero mid-sale, allowing the owner to reconfigure sale parameters after tokens have already been sold
- Could be used to front-run or manipulate sale timing
- Potential for owner to abuse by repeatedly resetting to extend sale duration

**Mitigations**:
- Protected by #[only_owner] attribute, requiring contract owner authorization
- Event emission for audit trail (can be monitored on-chain)
- Should only be used in genuine emergency situations
- Consider adding time-lock or multi-sig requirement for production deployment

**Recommendation**: Document emergency use cases and implement governance controls for production.

#### set_test_mode (Crowdsale Contract)
**Purpose**: Testnet-only function to skip token transfers during testing, allowing faster test execution without actual token movements.

**Risks**:
- If enabled in production, allows bypassing token transfer logic, potentially enabling unauthorized token minting or transfer manipulation
- Could be used to circumvent token balance checks and transfer validations
- No compile-time restriction (not gated with #[cfg(test)]), making it accessible in production builds

**Mitigations**:
- Protected by #[only_owner] attribute, requiring contract owner authorization
- Warning comments in code indicating production risks
- Should never be called in production environments

**Mainnet Deployment Plan**: **REMOVE this function before mainnet deployment**. Replace with proper testnet deployment script that uses actual token transfers.

### Mainnet Deployment Checklist

Before deploying to mainnet, ensure the following items are completed:

- [ ] **Remove set_test_mode function** from crowdsale contract
- [ ] **Remove is_test_mode function** from crowdsale contract
- [ ] **Remove test mode storage key** initialization
- [ ] **Verify constructor-based initialization** is working correctly
- [ ] **Verify oracle configuration** is properly set for production
- [ ] **Verify emergency_reset_sale** has proper governance controls (time-lock or multi-sig)
- [ ] **Audit coverage report** meets 99% line coverage target
- [ ] **Run all adversarial tests** and verify they pass
- [ ] **Review and test** all state transitions
- [ ] **Verify whitelist enforcement** is properly configured
- [ ] **Verify per-distribution fund tracking** is working
- [ ] **Verify token validation** in fund_payout_token
- [ ] **Verify TTL extension** on compliance-critical keys
- [ ] **Verify contract version field** is set correctly
- [ ] **Conduct final security audit** of production-ready code
- [ ] **Deploy to testnet** and run full integration tests
- [ ] **Document all contract IDs** and transaction hashes
- [ ] **Prepare incident response plan** for emergency situations

### Audit History
- [Date]: Initial security audit by [Auditor]
- [Date]: Security review following [Vulnerability] fix

## Security Updates

Security updates will be announced through:
- GitHub Security Advisories
- Official project communication channels
- Community forums and Discord

## Responsible Disclosure Policy

We follow a responsible disclosure policy:
- Do not exploit discovered vulnerabilities
- Allow reasonable time for remediation before public disclosure
- Work with us to ensure proper credit for findings

## Security Contacts

- **Security Team**: security@simplytokenized.com
- **Technical Lead**: [Contact Information]
- **Project Maintainer**: [Contact Information]

## Legal

This security policy is provided as-is and may be updated at any time. By reporting vulnerabilities, you agree to follow this policy.
