class CreateTransactions < ActiveRecord::Migration[5.2]
  def change
    create_table :transactions do |t|
      t.integer :account_id, null: false
      t.decimal :amount, precision: 8, scale: 2, null: false
      t.text :notes
      t.datetime :date, null: false
      t.datetime :created_at, null: false
      t.datetime :updated_at, null: false
      t.string :description
      t.string :category
      
      t.timestamps
    end
    add_index "transactions", [:account_id], name: :index_transactions_on_account_id, using: :btree
  end
end
