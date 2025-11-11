export interface Container {
  id: string;
  name: string;
  image: string;
  status: string;
  created: number;
  ports: string[];
  networks: string[];
}

export interface Network {
  id: string;
  name: string;
  driver: string;
  containers: string[];
}

export interface ApiResponse<T> {
  success: boolean;
  data: T;
  message?: string;
}