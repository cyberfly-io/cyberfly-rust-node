import { useState } from 'react';
import { dialPeer } from '../api/client';

export default function PeerConnection() {
  const [peerId, setPeerId] = useState('');
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<{ success: boolean; message: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleConnect = async (e: React.FormEvent) => {
    e.preventDefault();
    
    if (!peerId.trim()) {
      setError('Please enter a peer ID');
      return;
    }

    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const response = await dialPeer(peerId.trim());
      setResult(response);
      if (response.success) {
        // Clear form on success
        setPeerId('');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to connect to peer');
    } finally {
      setLoading(false);
    }
  };

  const handleClear = () => {
    setPeerId('');
    setResult(null);
    setError(null);
  };

  return (
    <div className="max-w-4xl mx-auto">
      <div className="bg-white dark:bg-gray-800 dark:bg-gray-800 rounded-lg shadow-md p-6">
        <h2 className="text-2xl font-bold mb-6 text-gray-800 dark:text-gray-200">Connect to Peer</h2>

        <div className="bg-blue-50 border border-blue-200 rounded-lg p-4 mb-6">
          <h3 className="font-semibold text-blue-900 mb-2">About Peer Connections</h3>
          <p className="text-sm text-blue-800 mb-2">
            Enter a peer's EndpointId (public key) to establish a direct connection. This uses Iroh's 
            hole-punching to create peer-to-peer connections.
          </p>
          <p className="text-sm text-blue-700">
            <strong>Format:</strong> A 64-character hexadecimal string (e.g., the EndpointId shown in your Dashboard)
          </p>
        </div>

        <form onSubmit={handleConnect} className="space-y-4">
          <div>
            <label htmlFor="peerId" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Peer ID (EndpointId)
            </label>
            <input
              id="peerId"
              type="text"
              value={peerId}
              onChange={(e) => setPeerId(e.target.value)}
              placeholder="Enter peer's EndpointId (64-char hex string)"
              className="w-full px-3 py-2 rounded-md border border-gray-300 bg-white text-gray-900 focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono text-sm"
              disabled={loading}
            />
          </div>

          <div className="flex gap-3">
            <button
              type="submit"
              disabled={loading || !peerId.trim()}
              className="flex-1 bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors"
            >
              {loading ? (
                <span className="flex items-center justify-center">
                  <svg className="animate-spin -ml-1 mr-3 h-5 w-5 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                  Connecting...
                </span>
              ) : (
                'Connect to Peer'
              )}
            </button>
            
            <button
              type="button"
              onClick={handleClear}
              disabled={loading}
              className="px-4 py-2 border border-gray-300 dark:border-gray-600 dark:border-gray-600 rounded-md text-gray-700 dark:text-gray-300 dark:text-gray-300 hover:bg-gray-50 dark:bg-gray-700 dark:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              Clear
            </button>
          </div>
        </form>

        {/* Success Message */}
        {result && result.success && (
          <div className="mt-6 p-4 bg-green-50 border border-green-200 rounded-lg">
            <div className="flex items-start">
              <svg className="w-5 h-5 text-green-600 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
              </svg>
              <div className="ml-3">
                <h3 className="text-sm font-medium text-green-800">Connection Successful</h3>
                <p className="mt-1 text-sm text-green-700">{result.message}</p>
              </div>
            </div>
          </div>
        )}

        {/* Failure Message */}
        {result && !result.success && (
          <div className="mt-6 p-4 bg-yellow-50 border border-yellow-200 rounded-lg">
            <div className="flex items-start">
              <svg className="w-5 h-5 text-yellow-600 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
              </svg>
              <div className="ml-3">
                <h3 className="text-sm font-medium text-yellow-800">Connection Failed</h3>
                <p className="mt-1 text-sm text-yellow-700">{result.message}</p>
              </div>
            </div>
          </div>
        )}

        {/* Error Message */}
        {error && (
          <div className="mt-6 p-4 bg-red-50 border border-red-200 rounded-lg">
            <div className="flex items-start">
              <svg className="w-5 h-5 text-red-600 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
              </svg>
              <div className="ml-3">
                <h3 className="text-sm font-medium text-red-800">Error</h3>
                <p className="mt-1 text-sm text-red-700">{error}</p>
              </div>
            </div>
          </div>
        )}

        {/* Info Section */}
        <div className="mt-8 border-t border-gray-200 dark:border-gray-700 pt-6">
          <h3 className="text-lg font-semibold text-gray-800 dark:text-gray-200 mb-3">How to Find Peer IDs</h3>
          <ul className="space-y-2 text-sm text-gray-600 dark:text-gray-400 dark:text-gray-400">
            <li className="flex items-start">
              <span className="mr-2">•</span>
              <span>Your own EndpointId is displayed in the Dashboard under "Node Information"</span>
            </li>
            <li className="flex items-start">
              <span className="mr-2">•</span>
              <span>Other peers' EndpointIds can be found in the "Discovered Peers" section</span>
            </li>
            <li className="flex items-start">
              <span className="mr-2">•</span>
              <span>EndpointIds are 64-character hexadecimal strings (256-bit public keys)</span>
            </li>
            <li className="flex items-start">
              <span className="mr-2">•</span>
              <span>Successful connections are tracked and displayed in the Dashboard</span>
            </li>
          </ul>
        </div>
      </div>
    </div>
  );
}
