import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { Upload, Download, File, Loader2 } from 'lucide-react';
import { uploadBlob, downloadBlob } from '../api/client';

export default function BlobManager() {
  const [file, setFile] = useState<File | null>(null);
  const [uploadResult, setUploadResult] = useState<{ hash?: string; error?: string }>({});
  const [downloadHash, setDownloadHash] = useState('');

  const uploadMutation = useMutation({
    mutationFn: uploadBlob,
    onSuccess: (hash) => {
      setUploadResult({ hash });
      setFile(null);
    },
    onError: (error: any) => {
      setUploadResult({ error: error.message || 'Upload failed' });
    },
  });

  const downloadMutation = useMutation({
    mutationFn: downloadBlob,
    onSuccess: (blob) => {
      // Create download link
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `blob-${downloadHash}`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    },
  });

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files && e.target.files[0]) {
      setFile(e.target.files[0]);
      setUploadResult({});
    }
  };

  const handleUpload = () => {
    if (file) {
      uploadMutation.mutate(file);
    }
  };

  const handleDownload = () => {
    if (downloadHash) {
      downloadMutation.mutate(downloadHash);
    }
  };

  return (
    <div className="p-6">
      <h1 className="text-3xl font-bold text-gray-900 mb-6">Blob Storage</h1>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Upload Section */}
        <div className="bg-white rounded-lg shadow p-6">
          <h2 className="text-xl font-semibold mb-4 flex items-center gap-2">
            <Upload className="w-5 h-5" />
            Upload Blob
          </h2>

          <div className="space-y-4">
            <div className="border-2 border-dashed border-gray-300 rounded-lg p-8 text-center">
              {file ? (
                <div className="flex items-center justify-center gap-3">
                  <File className="w-8 h-8 text-blue-600" />
                  <div className="text-left">
                    <p className="font-medium text-gray-900">{file.name}</p>
                    <p className="text-sm text-gray-500">
                      {(file.size / 1024).toFixed(2)} KB
                    </p>
                  </div>
                </div>
              ) : (
                <div>
                  <Upload className="w-12 h-12 text-gray-400 mx-auto mb-3" />
                  <p className="text-gray-600">Click to select a file</p>
                </div>
              )}
              <input
                type="file"
                onChange={handleFileChange}
                className="hidden"
                id="file-upload"
              />
              <label
                htmlFor="file-upload"
                className="mt-4 inline-block px-4 py-2 bg-gray-100 hover:bg-gray-200 rounded-md cursor-pointer transition"
              >
                Choose File
              </label>
            </div>

            <button
              onClick={handleUpload}
              disabled={!file || uploadMutation.isPending}
              className="w-full flex items-center justify-center gap-2 px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:bg-gray-400 disabled:cursor-not-allowed transition"
            >
              {uploadMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Uploading...
                </>
              ) : (
                <>
                  <Upload className="w-4 h-4" />
                  Upload to Iroh
                </>
              )}
            </button>

            {uploadResult.hash && (
              <div className="p-4 bg-green-50 border border-green-200 rounded-md">
                <p className="text-sm font-medium text-green-900 mb-2">Upload successful!</p>
                <code className="block text-xs text-green-800 break-all">
                  Hash: {uploadResult.hash}
                </code>
              </div>
            )}

            {uploadResult.error && (
              <div className="p-4 bg-red-50 border border-red-200 rounded-md">
                <p className="text-sm text-red-800">{uploadResult.error}</p>
              </div>
            )}
          </div>
        </div>

        {/* Download Section */}
        <div className="bg-white rounded-lg shadow p-6">
          <h2 className="text-xl font-semibold mb-4 flex items-center gap-2">
            <Download className="w-5 h-5" />
            Download Blob
          </h2>

          <div className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-2">
                Blob Hash
              </label>
              <input
                type="text"
                value={downloadHash}
                onChange={(e) => setDownloadHash(e.target.value)}
                placeholder="Enter Iroh blob hash"
                className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 font-mono text-sm"
              />
            </div>

            <button
              onClick={handleDownload}
              disabled={!downloadHash || downloadMutation.isPending}
              className="w-full flex items-center justify-center gap-2 px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 disabled:bg-gray-400 disabled:cursor-not-allowed transition"
            >
              {downloadMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Downloading...
                </>
              ) : (
                <>
                  <Download className="w-4 h-4" />
                  Download from Iroh
                </>
              )}
            </button>

            {downloadMutation.isError && (
              <div className="p-4 bg-red-50 border border-red-200 rounded-md">
                <p className="text-sm text-red-800">
                  {(downloadMutation.error as any)?.message || 'Download failed'}
                </p>
              </div>
            )}
          </div>

          {/* Info */}
          <div className="mt-6 p-4 bg-blue-50 border border-blue-200 rounded-md">
            <h3 className="font-medium text-blue-900 mb-2">About Iroh Blobs</h3>
            <ul className="text-sm text-blue-800 space-y-1">
              <li>• Content-addressed storage (hash-based)</li>
              <li>• Automatic deduplication</li>
              <li>• P2P distribution across nodes</li>
              <li>• Verifiable integrity via hash</li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
