class CreateTransactions < ActiveRecord::Migration[5.2]
  def change
    create_table :transactions do |t|
      t.integer :account_id, null: false
      t.float :amount, null: false
      t.string :description, null: false
      t.string :transaction_category, null:false
      t.string :tags
      t.string :date
      t.timestamps
    end
  end
end
