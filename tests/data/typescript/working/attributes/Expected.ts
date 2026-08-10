export class Grid {
  userAgent: string;
  apiKey: string;
  strategy: string | null;
  mission = new Mission(this);
}
