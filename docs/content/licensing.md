# Licensing

Fortemi is licensed under the **Business Source License 1.1 (BSL 1.1)** with a change date of **January 30, 2030**, after which it converts to **AAGPL v3** (GNU Affero General Public License).

This document explains what that means in plain English.

## License Overview

The Business Source License (BSL) is a "eventually open source" license that:

1. **Allows free use** for non-production purposes (personal, educational, evaluation, development)
2. **Restricts production use** to licensed deployments
3. **Automatically converts** to AGPL v3 open source after the change date (January 30, 2030)

Think of it as "free for individuals, paid for companies running it as infrastructure, and fully open source in 4 years."

## What You Can Do

You are **free to use Fortemi** without a license for:

- **Personal projects** - Run it on your own computer/server for your own notes
- **Educational purposes** - Use it in classes, research, teaching
- **Evaluation** - Test it out to see if it fits your needs
- **Desktop use** - Run it locally for personal productivity
- **Development** - Fork, modify, experiment, contribute back
- **Internal tools** - Use it at work on your laptop for your own productivity

No license required. No questions asked.

## What Requires a License

You need a **commercial license** if you:

- **Deploy on company servers** for multiple users
- **Offer it as a service** to customers (hosted notes/knowledge base)
- **Embed it in a commercial product** that you sell or license
- **Run it in production** for business operations

The key distinction: **single-user desktop use vs. multi-user production deployment**.

## FAQ

### Can I use this for personal projects?

**Yes.** Run it for yourself, your family, your hobbies. No license needed.

### Can I use this at work on my laptop?

**Yes.** If you're running it locally for your own productivity (like a note-taking app), that's personal use. No license needed.

### Can I deploy this on company servers?

**Requires a commercial license.** If you're deploying it for your team/organization on shared infrastructure, that's production use.

### Can I fork and modify the code?

**Yes.** You can fork, modify, and distribute your changes under the same BSL 1.1 license. Your fork inherits the same license terms.

### Can I contribute back to the project?

**Yes, please do!** Contributions are welcome. By contributing, you agree your code will be licensed under BSL 1.1 (and eventually AGPL v3).

### When does it become AGPL v3?

**January 30, 2030.** After that date, all versions (past and future) automatically convert to AGPL v3, which is fully open source with no production restrictions.

### What if I need production use now?

**Contact us for a commercial license.** See the Commercial Licensing section below.

### What about open source alternatives?

If BSL doesn't work for you and you need production deployment now:

- **Wait until 2030** when it converts to AGPL v3
- **Contact us** for a commercial license (may include source access)
- **Build your own** using similar open source components

### Can I use this in a SaaS product?

**Requires a commercial license.** Offering Fortemi (or a derivative) as a hosted service to customers is production use.

### Can I use this in an open source project?

**Yes, with caveats.** Your project can integrate with Fortemi via its API, but if you embed/bundle Fortemi, your project must respect the BSL production use restrictions (or wait until 2030 for GPL).

### What happens to my license after 2030?

**Your commercial license remains valid**, but after January 30, 2030, anyone can use Fortemi for production without a license (under AGPL v3).

## Commercial Licensing

Need a production deployment today? We offer commercial licenses.

### Contact Information

- **GitHub Issues:** [github.com/Fortemi/fortemi/issues](https://github.com/Fortemi/fortemi/issues)
- **Response time:** 2-3 business days

### License Tiers

We offer flexible licensing based on your needs:

| Tier | Use Case | Pricing |
|------|----------|---------|
| **Personal** | Free | Single-user, non-production use |
| **Team** | Contact us | Up to 50 users, single deployment |
| **Enterprise** | Contact us | Unlimited users, multiple deployments |
| **OEM/Embedded** | Contact us | Bundle in your product |

All commercial licenses include:

- Production deployment rights
- Email support
- Bug fixes and security patches
- Upgrade rights to new versions

### What to Include in Your Inquiry

When contacting us, please provide:

- **Company name** and size
- **Use case** (internal team knowledge base, customer-facing, embedded, etc.)
- **Number of users** (approximate)
- **Deployment model** (self-hosted, cloud, hybrid)
- **Timeline** (when do you need to deploy?)

We'll work with you to find a license that fits your needs and budget.

## Open Source Conversion

### Change Date: January 30, 2030

On this date, Fortemi automatically converts from BSL 1.1 to **AGPL v3**.

This means:

- **No more production restrictions** - Anyone can deploy for any purpose
- **Full AGPL v3 freedoms** - Use, modify, distribute freely
- **Copyleft requirement** - Derivatives must be AGPL v3
- **All versions included** - Past and future releases are AGPL v3

### Why BSL → GPL?

We chose this path because:

1. **Sustain development** - Commercial licenses fund full-time development
2. **Prevent parasitic use** - Large companies can't just take and run for free
3. **Guarantee open source** - It WILL become fully open source, period
4. **Fair compromise** - Free for individuals, paid for businesses, open for everyone eventually

### What is AGPL v3?

The GNU Affero General Public License v3 is a strong copyleft open source license that:

- Allows commercial use
- Requires sharing modifications
- Requires source disclosure for network/server use (closes the "SaaS loophole")
- Prevents patent restrictions
- Preserves user freedoms

The AGPL is like GPL but with an additional requirement: if you run modified AGPL software as a network service, you must make the source code available to users of that service.

Learn more: https://www.gnu.org/licenses/agpl-3.0.html

## License Text

The license files are in the repository root:

- `LICENSE` - BSL 1.1 terms (current license until Change Date)
- `LICENSE.txt` - AGPL v3 full text (takes effect after Change Date)

Key parameters:

- **Licensor:** Fortémi Project
- **Licensed Work:** Fortémi (all versions)
- **Change Date:** January 30, 2030
- **Change License:** AGPL v3
- **Additional Use Grant:** Non-production use is free

## Summary

- **Free for personal use** - Run it on your laptop, experiment, learn
- **Paid for production** - Company deployments need a commercial license
- **Open source in 2030** - Converts to AGPL v3 automatically
- **Fair and sustainable** - Funds development while guaranteeing eventual open source

Questions? [Open an issue on GitHub](https://github.com/Fortemi/fortemi/issues).

## See Also

- [Release Process](releasing.md) - How we version and release
- [Architecture](architecture.md) - Technical overview
- [Contributing](../../CONTRIBUTING.md) - How to contribute code
