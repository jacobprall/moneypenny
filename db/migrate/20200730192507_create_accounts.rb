class CreateAccounts < ActiveRecord::Migration[5.2]
  def change
    create_table :accounts do |t|
      t.string :name, null: false
      t.string   :institution_id, null: false
      t.string   :user_id, null: false
      t.decimal  :balance, precision: 8, scale: 2, null: false
      t.string   :account_type, null: false
      t.datetime :created_at
      t.datetime :updated_at
      t.timestamps
    end
    add_index "accounts", ["institution_id"], name: "index_accounts_on_institution_id", using: :btree
    add_index "accounts", ["user_id"], name: "index_accounts_on_user_id", using: :btree
  end
end
