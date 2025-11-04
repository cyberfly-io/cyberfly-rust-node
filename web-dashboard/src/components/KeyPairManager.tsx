import { useState, useEffect } from 'react';
import { Key, Download, Trash2, RefreshCw } from 'lucide-react';
import { generateKeyPair, saveKeyPair, loadKeyPair, deleteKeyPair } from '../utils/crypto';

export const KeyPairManager = () => {
  const [keyPair, setKeyPair] = useState<{ publicKey: string; secretKey: string } | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [showSecretKey, setShowSecretKey] = useState(false);

  useEffect(() => {
    loadExistingKeyPair();
  }, []);

  const loadExistingKeyPair = () => {
    setIsLoading(true);
    const existing = loadKeyPair();
    setKeyPair(existing);
    setIsLoading(false);
  };

  const handleGenerate = () => {
    const newKeyPair = generateKeyPair();
    saveKeyPair(newKeyPair);
    setKeyPair(newKeyPair);
    setShowSecretKey(false);
  };

  const handleDelete = () => {
    if (window.confirm('Are you sure you want to delete your keypair? This action cannot be undone!')) {
      deleteKeyPair();
      setKeyPair(null);
      setShowSecretKey(false);
    }
  };

  const handleExport = () => {
    if (!keyPair) return;
    
    const dataStr = JSON.stringify(keyPair, null, 2);
    const blob = new Blob([dataStr], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    
    const link = document.createElement('a');
    link.href = url;
    link.download = `cyberfly-keypair-${Date.now()}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  const handleImport = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        const imported = JSON.parse(e.target?.result as string);
        if (imported.publicKey && imported.secretKey) {
          saveKeyPair(imported);
          setKeyPair(imported);
          setShowSecretKey(false);
        } else {
          alert('Invalid keypair file format');
        }
      } catch (error) {
        alert('Failed to import keypair: ' + (error as Error).message);
      }
    };
    reader.readAsText(file);
  };

  if (isLoading) {
    return (
      <div className="p-6">
        <div className="bg-white dark:bg-gray-800 dark:bg-gray-800 rounded-lg shadow p-6">
          <div className="flex items-center justify-center">
            <RefreshCw className="animate-spin mr-2" size={20} />
            Loading keypair...
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-6 pr-48">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white dark:text-gray-100">Ed25519 KeyPair Manager</h1>
        {keyPair && (
          <div className="flex gap-2">
            <button
              onClick={handleExport}
              className="flex items-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition"
            >
              <Download size={16} />
              Export
            </button>
            <button
              onClick={handleDelete}
              className="flex items-center gap-2 px-4 py-2 bg-red-600 text-white rounded-md hover:bg-red-700 transition"
            >
              <Trash2 size={16} />
              Delete
            </button>
          </div>
        )}
      </div>

      <div className="bg-white dark:bg-gray-800 dark:bg-gray-800 rounded-lg shadow p-6">{!keyPair ? (
        <div className="text-center py-8">
          <div className="flex justify-center mb-4">
            <Key className="text-gray-600 dark:text-gray-400 dark:text-gray-400" size={64} />
          </div>
          <p className="text-gray-600 dark:text-gray-400 dark:text-gray-400 mb-6">
            No keypair found. Generate a new Ed25519 keypair to sign your data submissions.
          </p>
          <div className="flex gap-4 justify-center">
            <button
              onClick={handleGenerate}
              className="flex items-center gap-2 px-6 py-3 bg-green-600 text-white rounded-md hover:bg-green-700 transition font-semibold"
            >
              <Key size={20} />
              Generate New KeyPair
            </button>
            <label className="flex items-center gap-2 px-6 py-3 bg-gray-600 text-white rounded-md hover:bg-gray-700 transition font-semibold cursor-pointer">
              <Download size={20} />
              Import KeyPair
              <input
                type="file"
                accept=".json"
                onChange={handleImport}
                className="hidden"
              />
            </label>
          </div>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="bg-gray-50 dark:bg-gray-700 dark:bg-gray-700 p-4 rounded-md">
            <label className="block text-sm font-semibold text-gray-700 dark:text-gray-300 dark:text-gray-300 mb-2">
              Public Key (Share this)
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={keyPair.publicKey}
                readOnly
                className="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 dark:border-gray-600 rounded-md bg-white dark:bg-gray-800 dark:bg-gray-800 text-gray-900 dark:text-white dark:text-gray-100 font-mono text-sm"
              />
              <button
                onClick={() => {
                  navigator.clipboard.writeText(keyPair.publicKey);
                  alert('Public key copied to clipboard!');
                }}
                className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition"
              >
                Copy
              </button>
            </div>
          </div>

          <div className="bg-gray-50 dark:bg-gray-700 dark:bg-gray-700 p-4 rounded-md">
            <div className="flex items-center justify-between mb-2">
              <label className="block text-sm font-semibold text-gray-700 dark:text-gray-300 dark:text-gray-300">
                Secret Key (Keep this private!)
              </label>
              <button
                onClick={() => setShowSecretKey(!showSecretKey)}
                className="text-sm text-blue-600 hover:text-blue-700 font-medium"
              >
                {showSecretKey ? 'Hide' : 'Show'}
              </button>
            </div>
            <div className="flex gap-2">
              <input
                type={showSecretKey ? 'text' : 'password'}
                value={keyPair.secretKey}
                readOnly
                className="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 dark:border-gray-600 rounded-md bg-white dark:bg-gray-800 dark:bg-gray-800 text-gray-900 dark:text-white dark:text-gray-100 font-mono text-sm"
              />
              <button
                onClick={() => {
                  if (window.confirm('Are you sure you want to copy the secret key? Keep it safe!')) {
                    navigator.clipboard.writeText(keyPair.secretKey);
                    alert('Secret key copied to clipboard!');
                  }
                }}
                className="px-4 py-2 bg-yellow-600 text-white rounded-md hover:bg-yellow-700 transition"
              >
                Copy
              </button>
            </div>
            <p className="text-xs text-red-600 mt-2">
              ⚠️ Never share your secret key! Anyone with this key can sign data as you.
            </p>
          </div>

          <div className="bg-blue-50 border border-blue-200 p-4 rounded-md">
            <h3 className="text-sm font-semibold text-blue-900 mb-2">
              ℹ️ Usage Information
            </h3>
            <ul className="text-sm text-blue-800 space-y-1">
              <li>• Your public key identifies you on the network</li>
              <li>• Your secret key is used to sign all data submissions</li>
              <li>• Keys are stored in your browser's localStorage</li>
              <li>• Export and backup your keypair to prevent loss</li>
              <li>• This keypair is automatically used in the Data Submit form</li>
            </ul>
          </div>

          <button
            onClick={handleGenerate}
            className="w-full flex items-center justify-center gap-2 px-6 py-3 bg-orange-600 text-white rounded-md hover:bg-orange-700 transition font-semibold"
          >
            <RefreshCw size={20} />
            Generate New KeyPair (Replace Current)
          </button>
        </div>
      )}
      </div>
    </div>
  );
};
