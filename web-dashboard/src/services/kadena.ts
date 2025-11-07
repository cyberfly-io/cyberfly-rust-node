import { createClient, Pact } from '@kadena/client';

const NETWORK = 'mainnet01';
const CHAIN_ID = '1';
const NETWORK_URL = `https://api.chainweb.com/chainweb/0.0/${NETWORK}/chain/${CHAIN_ID}/pact`;

const client = createClient(NETWORK_URL);

/**
 * Get current APY from Kadena smart contract
 */
export async function getAPY(): Promise<number | null> {
  try {
    const unsignedTransaction = Pact.builder
      .execution(`(free.cyberfly_node.calculate-apy)`)
      .setMeta({
        chainId: CHAIN_ID,
        senderAccount: 'cyberfly-account-gas',
        gasLimit: 150000,
      })
      .setNetworkId(NETWORK)
      .createTransaction();

    const res = await client.local(unsignedTransaction, {
      signatureVerification: false,
      preflight: false,
    });

    if (res.result.status === 'success') {
      return res.result.data as number;
    }
    
    console.error('Failed to get APY:', res.result);
    return null;
  } catch (error) {
    console.error('Error fetching APY:', error);
    return null;
  }
}

/**
 * Get stake statistics from Kadena smart contract
 */
export async function getStakeStats(): Promise<{ totalStakes: number; activeStakes: number } | null> {
  try {
    const unsignedTransaction = Pact.builder
      .execution(`(free.cyberfly_node.get-stakes-stats)`)
      .setMeta({
        chainId: CHAIN_ID,
        senderAccount: 'cyberfly-account-gas',
        gasLimit: 150000,
      })
      .setNetworkId(NETWORK)
      .createTransaction();

    const res = await client.local(unsignedTransaction, {
      signatureVerification: false,
      preflight: false,
    });

    if (res.result.status === 'success') {
      const data = res.result.data as any;
      return {
        totalStakes: data['total-stakes'] || data.totalStakes || 0,
        activeStakes: data['active-stakes'] || data.activeStakes || 0,
      };
    }
    
    console.error('Failed to get stake stats:', res.result);
    return null;
  } catch (error) {
    console.error('Error fetching stake stats:', error);
    return null;
  }
}
