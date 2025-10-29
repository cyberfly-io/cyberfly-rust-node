import { useState, useEffect } from 'react';
import { useMutation } from '@tanstack/react-query';
import { Send, Loader2 } from 'lucide-react';
import { submitData, type DataSubmission } from '../api/client';
import { loadKeyPair, signData } from '../utils/crypto';

export default function DataSubmit() {
  const [storeType, setStoreType] = useState<DataSubmission['storeType']>('String');
  const [key, setKey] = useState('');
  const [value, setValue] = useState('');
  const [keyPair, setKeyPair] = useState<{ publicKey: string; secretKey: string } | null>(null);
  const [result, setResult] = useState<{ success?: string; error?: string }>({});

  // Load keypair on mount
  useEffect(() => {
    const loaded = loadKeyPair();
    setKeyPair(loaded);
  }, []);

  const submitMutation = useMutation({
    mutationFn: submitData,
    onSuccess: (message) => {
      setResult({ success: `Data submitted successfully! ${message}` });
      // Clear form
      setKey('');
      setValue('');
    },
    onError: (error: any) => {
      setResult({ error: error.message || 'Failed to submit data' });
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setResult({});

    if (!keyPair) {
      setResult({ error: 'No keypair found! Please generate a keypair first in the KeyPair page.' });
      return;
    }

    let parsedValue: any = value;
    
    // Parse value based on store type
    try {
      if (['Hash', 'Json', 'Geo'].includes(storeType)) {
        parsedValue = JSON.parse(value);
      } else if (storeType === 'List') {
        parsedValue = value.split(',').map(v => v.trim());
      } else if (storeType === 'Set') {
        parsedValue = Array.from(new Set(value.split(',').map(v => v.trim())));
      } else if (storeType === 'SortedSet') {
        parsedValue = JSON.parse(value);
      }
    } catch (err) {
      setResult({ error: 'Invalid value format for selected store type' });
      return;
    }

    // Generate dbName
    const dbName = `mydb-${keyPair.publicKey}`;
    
    // Convert value to JSON string (same format as will be sent to backend)
    const valueStr = typeof parsedValue === 'string' ? parsedValue : JSON.stringify(parsedValue);

    // Sign the data in the exact format the backend expects: "dbName:key:value"
    const messageToSign = `${dbName}:${key}:${valueStr}`;

    // Sign the data
    try {
      const signature = signData(messageToSign, keyPair.secretKey);

      submitMutation.mutate({
        storeType,
        key,
        value: parsedValue,
        publicKey: keyPair.publicKey,
        signature,
        timestamp: Date.now(),
        dbName,
      });
    } catch (err) {
      setResult({ error: 'Failed to sign data: ' + (err as Error).message });
    }
  };

  const getValuePlaceholder = () => {
    switch (storeType) {
      case 'String':
        return 'Enter string value';
      case 'Hash':
        return '{"field1": "value1", "field2": "value2"}';
      case 'List':
        return 'item1, item2, item3';
      case 'Set':
        return 'unique1, unique2, unique3';
      case 'SortedSet':
        return '[{"score": 1.0, "data": "value"}]';
      case 'Json':
        return '{"any": "json", "structure": true}';
      case 'Stream':
        return '{"event": "sensor_reading", "value": 23.5}';
      case 'TimeSeries':
        return '{"timestamp": 1234567890, "value": 42.0}';
      case 'Geo':
        return '{"lat": 37.7749, "lon": -122.4194, "label": "SF"}';
      default:
        return 'Enter value';
    }
  };

  return (
    <div className="p-6">
      <h1 className="text-3xl font-bold text-gray-900 mb-6">Store Data</h1>

      <div className="bg-white rounded-lg shadow p-6 max-w-2xl">
        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Store Type */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Store Type
            </label>
            <select
              value={storeType}
              onChange={(e) => setStoreType(e.target.value as DataSubmission['storeType'])}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            >
              <option value="String">String</option>
              <option value="Hash">Hash</option>
              <option value="List">List</option>
              <option value="Set">Set</option>
              <option value="SortedSet">Sorted Set</option>
              <option value="Json">JSON</option>
              <option value="Stream">Stream</option>
              <option value="TimeSeries">Time Series</option>
              <option value="Geo">Geospatial</option>
            </select>
          </div>

          {/* Key */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Key
            </label>
            <input
              type="text"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="my_key"
              required
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
          </div>

          {/* Value */}
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Value
            </label>
            <textarea
              value={value}
              onChange={(e) => setValue(e.target.value)}
              placeholder={getValuePlaceholder()}
              required
              rows={4}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono text-sm"
            />
          </div>

          {/* KeyPair Status */}
          {keyPair ? (
            <div className="p-4 bg-green-50 border border-green-200 rounded-md">
              <p className="text-sm text-green-800 font-medium mb-1">✓ KeyPair Loaded</p>
              <p className="text-xs text-gray-600 font-mono break-all">
                Public Key: {keyPair.publicKey.substring(0, 16)}...
              </p>
              <p className="text-xs text-gray-500 mt-1">
                Data will be automatically signed with your keypair
              </p>
            </div>
          ) : (
            <div className="p-4 bg-yellow-50 border border-yellow-200 rounded-md">
              <p className="text-sm text-yellow-800 font-medium mb-1">⚠️ No KeyPair Found</p>
              <p className="text-xs text-gray-600">
                Please generate a keypair in the <a href="#" onClick={(e) => { e.preventDefault(); window.location.reload(); }} className="text-blue-600 hover:underline">KeyPair page</a> before storing data.
              </p>
            </div>
          )}

          {/* Submit Button */}
          <button
            type="submit"
            disabled={submitMutation.isPending || !keyPair}
            className="w-full flex items-center justify-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:bg-gray-400 disabled:cursor-not-allowed transition"
          >
            {submitMutation.isPending ? (
              <>
                <Loader2 className="w-4 h-4 animate-spin" />
                Storing...
              </>
            ) : (
              <>
                <Send className="w-4 h-4" />
                Store Data
              </>
            )}
          </button>

          {/* Result */}
          {result.success && (
            <div className="p-4 bg-green-50 border border-green-200 rounded-md">
              <p className="text-sm text-green-800">{result.success}</p>
            </div>
          )}
          {result.error && (
            <div className="p-4 bg-red-50 border border-red-200 rounded-md">
              <p className="text-sm text-red-800">{result.error}</p>
            </div>
          )}
        </form>

        {/* Help Text */}
        <div className="mt-6 p-4 bg-blue-50 border border-blue-200 rounded-md">
          <h3 className="font-medium text-blue-900 mb-2">How it works</h3>
          <p className="text-sm text-blue-800">
            Data is cryptographically signed with your Ed25519 keypair before storage. Your public key identifies the data owner, and the signature proves authenticity.
          </p>
        </div>
      </div>
    </div>
  );
}
