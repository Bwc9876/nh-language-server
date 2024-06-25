export type ShipLogEntry = {
    id: string;
    astroObject: string;
    position?: [number, number];
    name: string;
    parent?: string;
    isCuriosity: boolean;
    sources: string[];
    curiosity?: string;
};
