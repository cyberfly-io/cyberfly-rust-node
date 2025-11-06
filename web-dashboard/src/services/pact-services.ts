import { createClient, Pact, createSignWithEckoWallet } from '@kadena/client';

const POLL_INTERVAL_S = 5;
const network = 'mainnet01';
const chainId = '1';
const networkUrl = `https://api.chainweb.com/chainweb/0.0/${network}/chain/${chainId}/pact`;
const client = createClient(networkUrl);

// Node status and stake types
export interface NodeInfo {
  peer_id: string;
  status: string;
  multiaddr: string;
  account: string;
  guard: any;
  register_date?: string;
  last_active_date?: string;
}

export interface NodeStakeInfo {
  active: boolean;
  amount?: number;
  staker?: string;
}

export interface ClaimableReward {
  days: number;
  reward: number;
}

// Get single node information
export const getNode = async (peerId: string): Promise<NodeInfo> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.get-node "${peerId}")`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Get nodes owned by an account
export const getMyNodes = async (account: string): Promise<NodeInfo[]> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.get-account-nodes "${account}")`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 150000,
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Get all active nodes
export const getActiveNodes = async (): Promise<NodeInfo[]> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.get-all-active-nodes)`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 150000,
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Get APY
export const getAPY = async (): Promise<number> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.calculate-apy)`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 150000,
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Get stake statistics
export const getStakeStats = async (): Promise<any> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.get-stakes-stats)`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 150000,
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Get node stake information
export const getNodeStake = async (peerId: string): Promise<NodeStakeInfo> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.get-node-stake "${peerId}")`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 150000,
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Get claimable rewards for a node
export const getNodeClaimable = async (peerId: string): Promise<ClaimableReward> => {
  const unsignedTransaction = Pact.builder
    .execution(`(free.cyberfly_node.calculate-days-and-reward "${peerId}")`)
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 150000,
    })
    .setNetworkId(network)
    .createTransaction();

  const res = await client.local(unsignedTransaction, {
    signatureVerification: false,
    preflight: false,
  });
  return res.result.data;
};

// Stake on a node
export const nodeStake = async (account: string, peerId: string): Promise<any> => {
  const getPubkey = (account: string) => {
    return account.slice(2);
  };

  const utxn = Pact.builder
    .execution(`(free.cyberfly_node.stake "${account}" "${peerId}")`)
    .addSigner(getPubkey(account), (withCapability) => [
      withCapability('free.cyberfly-account-gas-station.GAS_PAYER', 'cyberfly-account-gas', { int: 1 }, 1.0),
      withCapability('free.cyberfly_node.ACCOUNT_AUTH', account),
      withCapability('free.cyberfly_node.NODE_GUARD', peerId),
      withCapability('free.cyberfly_token.TRANSFER', account, 'cyberfly-staking-bank', 50000.0),
    ])
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 2000,
      gasPrice: 0.0000001,
      ttl: 28000,
    })
    .setNetworkId(network)
    .createTransaction();

  const signTransaction = createSignWithEckoWallet();
  const signedTx = await signTransaction(utxn);
  const res = await client.local(signedTx);

  if (res.result.status === 'success') {
    const txn = await client.submit(signedTx);
    console.log('Stake transaction:', txn);
    // TODO: Poll for transaction completion
    return txn;
  } else {
    throw new Error(res.result.error?.message || 'Staking failed');
  }
};

// Unstake from a node
export const nodeUnStake = async (account: string, peerId: string): Promise<any> => {
  const getPubkey = (account: string) => {
    return account.slice(2);
  };

  const utxn = Pact.builder
    .execution(`(free.cyberfly_node.unstake "${account}" "${peerId}")`)
    .addSigner(getPubkey(account), (withCapability) => [
      withCapability('free.cyberfly-account-gas-station.GAS_PAYER', 'cyberfly-account-gas', { int: 1 }, 1.0),
      withCapability('free.cyberfly_node.ACCOUNT_AUTH', account),
    ])
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 2000,
      gasPrice: 0.0000001,
      ttl: 28000,
    })
    .setNetworkId(network)
    .createTransaction();

  const signTransaction = createSignWithEckoWallet();
  const signedTx = await signTransaction(utxn);
  const res = await client.local(signedTx);

  if (res.result.status === 'success') {
    const txn = await client.submit(signedTx);
    console.log('Unstake transaction:', txn);
    return txn;
  } else {
    throw new Error(res.result.error?.message || 'Unstaking failed');
  }
};

// Claim rewards
export const claimReward = async (account: string, peerId: string): Promise<any> => {
  const getPubkey = (account: string) => {
    return account.slice(2);
  };

  const utxn = Pact.builder
    .execution(`(free.cyberfly_node.claim-reward "${account}" "${peerId}")`)
    .addSigner(getPubkey(account), (withCapability) => [
      withCapability('free.cyberfly-account-gas-station.GAS_PAYER', 'cyberfly-account-gas', { int: 1 }, 1.0),
      withCapability('free.cyberfly_node.NODE_GUARD', peerId),
    ])
    .setMeta({
      chainId,
      senderAccount: 'cyberfly-account-gas',
      gasLimit: 2000,
      gasPrice: 0.0000001,
      ttl: 28000,
    })
    .setNetworkId(network)
    .createTransaction();

  const signTransaction = createSignWithEckoWallet();
  const signedTx = await signTransaction(utxn);
  const res = await client.local(signedTx);

  if (res.result.status === 'success') {
    const txn = await client.submit(signedTx);
    console.log('Claim transaction:', txn);
    return txn;
  } else {
    throw new Error(res.result.error?.message || 'Claim failed');
  }
};

// Poll for transaction result
export const pollForTransaction = async (
  requestKey: string,
  message: string,
  callback?: () => void
): Promise<void> => {
  let timeSpentPollingS = 0;
  let pollRes = null;

  while (timeSpentPollingS < 180) {
    // Max 3 minutes
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_S * 1000));
    timeSpentPollingS += POLL_INTERVAL_S;

    try {
      pollRes = await client.pollStatus({ requestKey }, {});
      if (pollRes[requestKey]) {
        console.log(`${message} - Transaction completed:`, pollRes[requestKey]);
        callback?.();
        break;
      }
    } catch (error) {
      console.error('Error polling transaction:', error);
    }
  }

  if (timeSpentPollingS >= 180) {
    console.warn(`${message} - Polling timed out after ${timeSpentPollingS}s`);
  }
};
