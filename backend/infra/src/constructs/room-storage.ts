import { RemovalPolicy } from "aws-cdk-lib";
import { BillingMode, Table, AttributeType } from "aws-cdk-lib/aws-dynamodb";
import { Construct } from "constructs";

export class RoomStorage extends Construct {
    readonly rooms: Table;
    readonly connections: Table;

    constructor(scope: Construct, id: string, stage: "dev" | "prod") {
        super(scope, id);
        const removalPolicy = stage === "prod" ? RemovalPolicy.RETAIN : RemovalPolicy.DESTROY;
        this.rooms = new Table(this, "Rooms", {
            partitionKey: { name: "table_id", type: AttributeType.STRING },
            billingMode: BillingMode.PAY_PER_REQUEST,
            timeToLiveAttribute: "ttl",
            removalPolicy,
        });
        this.connections = new Table(this, "Connections", {
            partitionKey: { name: "connection_id", type: AttributeType.STRING },
            billingMode: BillingMode.PAY_PER_REQUEST,
            timeToLiveAttribute: "ttl",
            removalPolicy,
        });
    }
}
