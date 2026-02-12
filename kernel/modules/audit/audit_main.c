/*
 * AGNOS Audit Kernel Module
 *
 * Provides tamper-evident audit logging with cryptographic
 * chain hashing for all security-relevant events.
 */

#include <linux/kernel.h>
#include <linux/module.h>
#include <linux/syscalls.h>
#include <linux/slab.h>
#include <linux/fs.h>
#include <linux/crypto.h>
#include <crypto/hash.h>
#include <crypto/hmac.h>

#define AUDIT_VERSION "0.1.0"
#define AUDIT_LOG_PATH "/var/log/agnos/audit.log"
#define HASH_SIZE 32  /* SHA256 */

/* Audit entry structure */
struct audit_entry {
    u64 sequence;
    u64 timestamp;
    uuid_t agent_id;
    uuid_t user_id;
    u32 action_type;
    u32 result;
    u8 hash[HASH_SIZE];
    u8 prev_hash[HASH_SIZE];
    u8 signature[HASH_SIZE];
    char payload[];
};

/* Audit context per task */
struct audit_context {
    u64 last_sequence;
    u8 last_hash[HASH_SIZE];
    bool enabled;
};

static atomic64_t audit_sequence = ATOMIC64_INIT(0);
static DEFINE_MUTEX(audit_mutex);
static struct file *audit_file;

/* Initialize audit chain with genesis hash */
static void init_audit_chain(struct audit_context *ctx)
{
    ctx->last_sequence = 0;
    /* Initialize with zeros - first entry will hash this */
    memset(ctx->last_hash, 0, HASH_SIZE);
    ctx->enabled = true;
}

/* Calculate SHA256 hash of audit entry */
static int hash_entry(struct audit_entry *entry, u8 *out_hash)
{
    struct crypto_shash *tfm;
    struct shash_desc *desc;
    int ret;
    
    tfm = crypto_alloc_shash("sha256", 0, 0);
    if (IS_ERR(tfm))
        return PTR_ERR(tfm);
    
    desc = kmalloc(sizeof(*desc) + crypto_shash_descsize(tfm), GFP_KERNEL);
    if (!desc) {
        crypto_free_shash(tfm);
        return -ENOMEM;
    }
    
    desc->tfm = tfm;
    
    ret = crypto_shash_init(desc);
    if (!ret) {
        ret = crypto_shash_update(desc, (u8 *)&entry->sequence, sizeof(entry->sequence));
        ret |= crypto_shash_update(desc, (u8 *)&entry->timestamp, sizeof(entry->timestamp));
        ret |= crypto_shash_update(desc, entry->prev_hash, HASH_SIZE);
        ret |= crypto_shash_update(desc, (u8 *)entry->payload, strlen(entry->payload));
        ret |= crypto_shash_final(desc, out_hash);
    }
    
    kfree(desc);
    crypto_free_shash(tfm);
    
    return ret;
}

/* Write audit entry to log */
static int write_audit_entry(struct audit_entry *entry)
{
    loff_t pos = 0;
    ssize_t written;
    
    if (!audit_file)
        return -ENODEV;
    
    /* TODO: Implement proper append-only write with fsync */
    
    written = kernel_write(audit_file, entry, sizeof(*entry) + strlen(entry->payload), &pos);
    
    return (written < 0) ? written : 0;
}

/* Main audit logging function */
int agnos_audit_log(struct audit_context *ctx, u32 action, void *data, int result)
{
    struct audit_entry *entry;
    size_t payload_len;
    int ret;
    
    if (!ctx || !ctx->enabled)
        return 0;
    
    /* For now, use simple string payload */
    payload_len = 256;  /* Max payload size */
    
    entry = kzalloc(sizeof(*entry) + payload_len, GFP_KERNEL);
    if (!entry)
        return -ENOMEM;
    
    mutex_lock(&audit_mutex);
    
    entry->sequence = atomic64_inc_return(&audit_sequence);
    entry->timestamp = ktime_get_real_ns();
    entry->action_type = action;
    entry->result = result;
    
    /* Copy previous hash for chain */
    memcpy(entry->prev_hash, ctx->last_hash, HASH_SIZE);
    
    /* Format payload */
    snprintf(entry->payload, payload_len, "action=%u,result=%d", action, result);
    
    /* Calculate hash of entry */
    ret = hash_entry(entry, entry->hash);
    if (ret) {
        mutex_unlock(&audit_mutex);
        kfree(entry);
        return ret;
    }
    
    /* TODO: Sign entry with kernel key */
    memset(entry->signature, 0, HASH_SIZE);
    
    /* Write to audit log */
    ret = write_audit_entry(entry);
    if (!ret) {
        /* Update context with new hash */
        memcpy(ctx->last_hash, entry->hash, HASH_SIZE);
        ctx->last_sequence = entry->sequence;
    }
    
    mutex_unlock(&audit_mutex);
    
    kfree(entry);
    return ret;
}
EXPORT_SYMBOL_GPL(agnos_audit_log);

static int __init audit_module_init(void)
{
    pr_info("AGNOS Audit Module v%s loading...\n", AUDIT_VERSION);
    
    /* Open audit log file */
    audit_file = filp_open(AUDIT_LOG_PATH, O_WRONLY | O_CREAT | O_APPEND, 0600);
    if (IS_ERR(audit_file)) {
        pr_warn("AGNOS Audit: Could not open audit log: %ld\n", PTR_ERR(audit_file));
        audit_file = NULL;
        /* Continue without file logging */
    }
    
    pr_info("AGNOS Audit Module loaded\n");
    return 0;
}

static void __exit audit_module_exit(void)
{
    if (audit_file) {
        filp_close(audit_file, NULL);
        audit_file = NULL;
    }
    
    pr_info("AGNOS Audit Module unloaded\n");
}

module_init(audit_module_init);
module_exit(audit_module_exit);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("AGNOS Team");
MODULE_DESCRIPTION("AGNOS Audit Kernel Module with cryptographic chain");
MODULE_VERSION(AUDIT_VERSION);
